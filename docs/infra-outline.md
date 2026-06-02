# Infrastructure Alternatives

> **NOTE:** The current primary production environment uses **Vast.ai** (see `infra/vast/` and `vast_deploy.sh`). 
> The AWS plan below is maintained as a high-availability alternative for H100 capacity.

## AWS GPU Spot Infrastructure: Implementation Plan

---

### Notes

I'm thinking based on the constraints, we probably want to aim for H100s.


### Architecture Decision: Hybrid Terraform + CLI

**Terraform:** VPC, subnets, IGW, route tables, SGs, IAM role/instance profile, S3, ElastiCache (optional). Everything that's persistent and stateful.

**AWS CLI:** Instance lifecycle. Spot instances and Terraform are a bad pairing — spot interruptions cause state drift, and the one-off launch/terminate workflow is actually *cleaner* with a raw CLI command or a short Ansible play. You're right to keep these separate.

---

### Internet Gateway Answer

You need the IGW. It's not just egress — it's the gateway for *all* internet traffic in/out of a public subnet, including your inbound SSH. Without it you'd need a bastion or VPN. With it: public subnet + public IP on the instance + SG locked to your IP = SSH works.

S3 gets a **VPC Gateway Endpoint** (free), so that traffic never hits the internet even though the instance is in a public subnet. ElastiCache is VPC-internal so it never touches the IGW at all.

---

### Terraform File Structure

```
terraform/
├── main.tf              # provider, locals
├── variables.tf         # region, your IP, bucket name, etc.
├── outputs.tf           # subnet ID, SG ID, instance profile name (for CLI use)
├── vpc.tf               # VPC, subnets, IGW, route tables, S3 VPC endpoint
├── security_groups.tf   # EC2 SG, ElastiCache SG
├── iam.tf               # EC2 instance role + profile, S3 policy
├── s3.tf                # bucket + policy
└── elasticache.tf       # optional, keep separate so you can toggle it
```

---

### Phase 1: VPC (`vpc.tf`)

```hcl
resource "aws_vpc" "main" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true   # needed for SSH by hostname if you want it
  enable_dns_support   = true
}

# Public subnets — one per AZ you might use
# GPU spot capacity is AZ-specific, so having 2-3 gives you options
resource "aws_subnet" "public" {
  for_each                = toset(["us-east-1a", "us-east-1b", "us-east-1c"])
  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet("10.0.0.0/16", 8, index(["us-east-1a","us-east-1b","us-east-1c"], each.key))
  availability_zone       = each.key
  map_public_ip_on_launch = true
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }
}

resource "aws_route_table_association" "public" {
  for_each       = aws_subnet.public
  subnet_id      = each.value.id
  route_table_id = aws_route_table.public.id
}

# Free — keeps S3 traffic off the internet and avoids data transfer charges
resource "aws_vpc_endpoint" "s3" {
  vpc_id            = aws_vpc.main.id
  service_name      = "com.amazonaws.${var.region}.s3"
  vpc_endpoint_type = "Gateway"
  route_table_ids   = [aws_route_table.public.id]
}
```

**Gotcha:** GPU spot availability is per-AZ and shifts constantly. Having subnets in 3 AZs means you can switch the `--subnet-id` argument at launch time without any infra changes.

---

### Phase 2: Security Groups (`security_groups.tf`)

```hcl
resource "aws_security_group" "ec2" {
  name   = "gpu-worker-ec2"
  vpc_id = aws_vpc.main.id

  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["${var.your_ip}/32"]  # your IP only
  }

  # Egress open — needed for Docker Hub pulls, apt, etc.
  # You could lock this down to 443 but it's not worth the maintenance pain
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "elasticache" {
  count  = var.enable_elasticache ? 1 : 0
  name   = "gpu-worker-redis"
  vpc_id = aws_vpc.main.id

  ingress {
    from_port       = 6379
    to_port         = 6379
    protocol        = "tcp"
    security_groups = [aws_security_group.ec2.id]
  }
  # No egress rule needed — Redis doesn't initiate connections
}
```

---

### Phase 3: IAM (`iam.tf`)

```hcl
data "aws_iam_policy_document" "ec2_assume" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ec2.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "ec2_worker" {
  name               = "gpu-worker-role"
  assume_role_policy = data.aws_iam_policy_document.ec2_assume.json
}

data "aws_iam_policy_document" "s3_worker" {
  statement {
    actions = [
      "s3:GetObject",
      "s3:PutObject",
      "s3:DeleteObject",
      "s3:ListBucket",
    ]
    resources = [
      aws_s3_bucket.work.arn,
      "${aws_s3_bucket.work.arn}/*",
    ]
  }
}

resource "aws_iam_role_policy" "s3_worker" {
  name   = "s3-work-access"
  role   = aws_iam_role.ec2_worker.id
  policy = data.aws_iam_policy_document.s3_worker.json
}

resource "aws_iam_instance_profile" "ec2_worker" {
  name = "gpu-worker-profile"
  role = aws_iam_role.ec2_worker.name
}
```

The instance profile name goes in `outputs.tf` so you can paste it directly into your CLI launch command. No credential management on the instance — the instance metadata service handles auth automatically.

---

### Phase 4: S3 (`s3.tf`)

```hcl
resource "aws_s3_bucket" "zkpow" {
  bucket = var.bucket_name
}

resource "aws_s3_bucket_public_access_block" "zkpow" {
  bucket = aws_s3_bucket.work.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# Your personal IAM user gets full access via the bucket policy
# No need for a separate IAM policy attachment if this is cleaner for you
data "aws_iam_policy_document" "bucket_policy" {
  # EC2 role access
  statement {
    principals {
      type        = "AWS"
      identifiers = [aws_iam_role.ec2_worker.arn]
    }
    actions   = ["s3:GetObject", "s3:PutObject", "s3:DeleteObject", "s3:ListBucket"]
    resources = [aws_s3_bucket.zkpow.arn, "${aws_s3_bucket.zkpow.arn}/*"]
  }

  # Your personal IAM user/role — full access for CLI and console
  statement {
    principals {
      type        = "AWS"
      identifiers = [var.your_iam_arn]  # e.g. "arn:aws:iam::123456789:user/tcrypt"
    }
    actions   = ["s3:*"]
    resources = [aws_s3_bucket.work.arn, "${aws_s3_bucket.work.arn}/*"]
  }
}

resource "aws_s3_bucket_policy" "work" {
  bucket = aws_s3_bucket.work.id
  policy = data.aws_iam_policy_document.bucket_policy.json
}
```

---

### Phase 5: ElastiCache (`elasticache.tf`)

```hcl
# Wrap in count = var.enable_elasticache ? 1 : 0 throughout
resource "aws_elasticache_subnet_group" "redis" {
  count      = var.enable_elasticache ? 1 : 0
  name       = "gpu-worker-redis"
  subnet_ids = values(aws_subnet.public)[*].id
}

resource "aws_elasticache_cluster" "redis" {
  count                = var.enable_elasticache ? 1 : 0
  cluster_id           = "gpu-worker-redis"
  engine               = "redis"
  node_type            = "cache.t4g.micro"   # cheapest; upgrade if you need throughput
  num_cache_nodes      = 1
  parameter_group_name = "default.redis7"
  port                 = 6379
  subnet_group_name    = aws_elasticache_subnet_group.redis[0].name
  security_group_ids   = [aws_security_group.elasticache[0].id]
}
```

No auth token needed — the SG is your access control. This is fine since nothing external can reach it.

---

### Phase 6: Variables + Outputs

**`variables.tf`:**
```hcl
variable "region"              { default = "us-east-1" }
variable "your_ip"             {}   # your current public IP, no default — force explicit
variable "your_iam_arn"        {}   # arn:aws:iam::ACCT:user/you
variable "bucket_name"         {}
variable "enable_elasticache"  { default = false }
```

**`outputs.tf`:**
```hcl
output "public_subnet_ids"      { value = { for k, v in aws_subnet.public : k => v.id } }
output "ec2_security_group_id"  { value = aws_security_group.ec2.id }
output "instance_profile_name"  { value = aws_iam_instance_profile.ec2_worker.name }
output "bucket_name"            { value = aws_s3_bucket.work.id }
output "redis_endpoint"         { value = var.enable_elasticache ? aws_elasticache_cluster.redis[0].cache_nodes[0].address : null }
```

These outputs are what you paste into your CLI launch command. No hunting through the console.

---

### Phase 7: Instance Launch (CLI)

After `terraform apply`, your launch command looks like:

```bash
aws ec2 run-instances \
  --region us-east-1 \
  --image-id ami-XXXXXXXX \               # Deep Learning AMI — see note below
  --instance-type g4dn.xlarge \           # T4 GPU, cheapest option
  --key-name your-key-pair \
  --security-group-ids sg-XXXXXXXX \      # from terraform output
  --subnet-id subnet-XXXXXXXX \           # pick AZ with spot availability
  --iam-instance-profile Name=gpu-worker-profile \
  --instance-market-options '{"MarketType":"spot","SpotOptions":{"SpotInstanceType":"one-time","InstanceInterruptionBehavior":"terminate"}}' \
  --block-device-mappings '[{"DeviceName":"/dev/sda1","Ebs":{"VolumeSize":50,"VolumeType":"gp3","DeleteOnTermination":true}}]' \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=gpu-worker}]'
```

`DeleteOnTermination: true` is critical — don't accumulate orphaned volumes.

---

### Things That Will Bite You If You Don't Handle Them Now

**GPU spot availability by AZ.** `g4dn` and `g5` capacity shifts constantly. If you get `InsufficientInstanceCapacity` on spot, it's almost always fixable by switching the subnet (and thus AZ) without any Terraform change. The multi-AZ subnet setup above is specifically for this.

**AMI selection.** Use the AWS Deep Learning AMI (Ubuntu). It has CUDA, nvidia drivers, Docker, and nvidia-container-toolkit pre-installed. Doing this yourself on a base Ubuntu AMI is a significant time sink. Find the current AMI ID with:
```bash
aws ec2 describe-images \
  --owners amazon \
  --filters "Name=name,Values=Deep Learning AMI GPU PyTorch*Ubuntu 22*" \
  --query 'sort_by(Images, &CreationDate)[-1].ImageId'
```

**Your IP in the SG will drift.** If your IP changes and you're locked out, you'll need to either update Terraform and re-apply, or temporarily add your new IP via the console. Consider keeping a narrow `/28` or `/27` for your ISP's range if this becomes annoying.

**IAM instance profile propagation delay.** There's a ~15-second delay between instance boot and the metadata service having valid credentials. If your startup script immediately tries to hit S3, add a small `sleep` or retry loop.

**Spot 2-minute termination notice.** Available at `http://169.254.169.254/latest/meta-data/spot/instance-action`. Your 50-55 minute self-stop strategy handles preemption gracefully as long as the checkpoint interval is short enough that losing 2 minutes of work is acceptable. If not, poll that endpoint in your runner loop and trigger an early checkpoint on the warning.

**ElastiCache and subnet group.** The subnet group just needs subnets in the same VPC — they don't need to be private. The SG handles isolation.

---

### Sequencing

1. Create key pair manually: `aws ec2 create-key-pair --key-name gpu-worker --query KeyMaterial --output text > ~/.ssh/gpu-worker.pem && chmod 600 ~/.ssh/gpu-worker.pem`
2. `terraform init && terraform plan` — review
3. `terraform apply` — ~2-3 min
4. Grab outputs
5. Find current DL AMI ID for your region
6. Run the `aws ec2 run-instances` command with the outputs plugged in
7. SSH, verify S3 access with `aws s3 ls s3://your-bucket` (no credentials needed — instance profile handles it), start Docker

The Terraform state here is small and low-churn — local state in the repo is fine. The only time you'd re-apply is if
your IP changes, you toggle ElastiCache on, or you add another bucket. None of that is frequent.
