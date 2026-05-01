# Comprehensive Market Analysis of Cloud Hosting

## Providers for zk-Proof Proving Projects

- AWS, GCP, and Azure dominate with broad GPU instance types, global regions, and mature automation support but face spot capacity issues.

- Specialized providers like CoreWeave, RunPod, and Lambda Labs offer competitive pricing, flexible spot markets, and zk-specific optimizations.

- Hetzner and OVHcloud provide cost-effective GPU servers with European focus but limited spot/preemptible options and regional coverage.

- Linode and DigitalOcean offer developer-friendly GPU instances with transparent pricing but limited spot capacity and smaller global footprint.

- Vast.ai’s bidding marketplace provides lowest-cost GPU access but with variable reliability and no guaranteed uptime.

## Executive Summary

For a zk-proof proving project requiring high-performance, cost-efficient, and flexible compute

resources—particularly NVIDIA GPUs (A100, H100, L40, etc.), high-memory CPUs, and low-

latency NVMe storage—the choice of cloud provider is critical. The analysis of major and

specialized cloud hosting providers reveals a nuanced landscape:

-

AWS, GCP, and Azure are the hyperscale leaders with extensive GPU instance

portfolios, global data center coverage, and robust automation (Terraform/Ansible/API).

They offer on-demand, spot/preemptible, and reserved pricing with discounts up to 90%

on spot instances. However, they suffer from frequent spot capacity shortages and

complex pricing that can lead to cost overruns. Their bare-metal and confidential

computing options enhance security but add complexity.

-

CoreWeave stands out for its focus on high-performance computing (HPC) and AI

workloads, offering bare-metal GPU instances with InfiniBand interconnects and

Kubernetes-native orchestration. It provides up to 60% reserved instance discounts and

is used by zk projects for its scalable, secure, and high-bandwidth clusters.

CoreWeave’s infrastructure is optimized for distributed proving workloads but requires

Kubernetes expertise.

-

RunPod offers a developer-centric GPU cloud with containerized environments, root

access, and transparent pricing starting at $0.34/GPU-hour for RTX 4090 and $1.99/

GPU-hour for H100. It supports Terraform and integrates with SkyPilot, enabling rapid

prototyping and distributed training. RunPod’s spot market and community cloud provide

cost-effective access but may face capacity constraints.

1/10

-

Lambda Labs provides on-demand H100 GPUs at ~$2.99/GPU-hour, targeting AI

researchers and enterprises needing high-end hardware with hybrid cloud options. Its

pricing is competitive with Azure but lacks spot/preemptible instances.

-

Vast.ai operates a competitive bidding marketplace where GPU prices start around

$0.90/hour for H100, enabling cost savings but with variable host reliability and no

uptime guarantees. Suitable for experimentation but not production workloads.

-

Hetzner offers dedicated GPU servers in Europe starting at €3.49/month, with no spot/

preemptible instances and limited GPU availability. Its low cost is attractive but lacks

flexibility and global coverage.

-

OVHcloud provides GPU instances optimized for AI and HPC workloads with European

data center presence but limited spot capacity and outdated hardware in some regions.

-

Linode and DigitalOcean focus on developer experience with straightforward pricing

and GPU droplets starting at $0.76/GPU-hour, but their spot capacity and global

footprint are smaller compared to hyperscalers.

Comparative Tables

Table 1: GPU Instances Overview

Provider

Instance

GPU

Name

Model

vCPU

RAM

Storage Type/

(/hr)|Spot/

Spot

(GB)

Size

PreemptiblePrice(/

Discount %

On-Demand Price

hr)

Reserved

Price

(1Y/3Y)

Terraform

Support

Custom

Image

Support

AWS

p4d.

NVIDIA

24xlarge

A100

96

1152

8x 1.9 TB

NVMe SSD

AWS

g5.48xlarge

NVIDIA

L40S

192

768

4x 1.9 TB

NVMe SSD

GCP

a2-

NVIDIA

highgpu-8g

H100

96

768

8x 3.8 TB

Local SSD

Azure

NC H100 v5

CoreWeave N/A

64

448

8x 1.9 TB

NVMe SSD

128

1024

NVMe

(customizable)

NVIDIA

H100

NVIDIA

H100

NVIDIA

RunPod

RTX 4090

RTX

16

64 NVMe

0.34

4090

2/10

32.77

19.66 (avg)

~80%

Yes

Yes

1.006

0.50 (avg)

~50%

Yes

Yes

6.69

6.98

~81%

Yes

Yes

~80%

Yes

Yes

1.24

(preemptible)

12.0 (8-GPU

ND H100 v5)

N/A

6.16

(reserved up

N/A

Yes

Yes

to 60% off)

N/A (spot

market)

N/A

Yes

Yes

Provider

Instance

GPU

Name

Model

vCPU

RAM

Storage Type/

(/hr)|Spot/

Spot

(GB)

Size

PreemptiblePrice(/

Discount %

On-Demand Price

hr)

Reserved

Price

(1Y/3Y)

Terraform

Support

Custom

Image

Support

64

256 NVMe

1.99

N/A (spot

market)

N/A

Yes

Yes

64

256 NVMe

2.99

N/A

N/A

Yes

Yes

64

256 NVMe

0.90

N/A (bidding

market)

N/A

No

No

Hetzner

GEX44

RTX

14

64

2x 1.92 TB

NVMe

0.0056 (€/hr)

N/A

N/A

No

No

16

64 NVMe

0.0075 (min)

N/A

N/A

Yes

Yes

64

256 NVMe

2.50

N/A

N/A

Yes

Yes

RunPod

H100

Lambda

Labs

H100

Vast.ai

H100

NVIDIA

H100

NVIDIA

H100

NVIDIA

H100

NVIDIA

4000

NVIDIA

L40S

NVIDIA

H100

Linode

GPU Linode

GPU

DigitalOcean

Droplet

H100

Table 2: High-RAM CPU Instances Overview

Provider

Instance

CPU

Name

Model

vCPU

RAM

Storage Type/

(/hr)|Spot/

Spot

(GB)

Size

PreemptiblePrice(/

Discount %

On-Demand Price

hr)

Reserved

Price

(1Y/3Y)

Terraform

Support

Custom

Image

Support

AWS

r6i.32xlarge

Intel

Xeon

128

1024 EBS/NVMe

3.04

0.91 (avg)

~70%

Yes

Yes

GCP

m2-

Intel

ultramem-128

Xeon

128

1536 Local SSD

1.20

0.36

(preemptible)

~70%

Yes

Yes

Azure

E16s v5

CoreWeave N/A

Intel

Xeon

AMD

EPYC

Linode

High Memory

Intel

128GB

Xeon

16

128 Premium SSD 0.624

0.187 (spot) ~70%

Yes

Yes

128

1024

NVMe

(customizable)

N/A

0.50

(reserved up

N/A

Yes

Yes

to 60% off)

16

128 NVMe

0.02

N/A

N/A

Yes

Yes

3/10

Provider

Instance

CPU

Name

Model

vCPU

RAM

Storage Type/

(/hr)|Spot/

Spot

(GB)

Size

PreemptiblePrice(/

Discount %

On-Demand Price

hr)

Reserved

Price

(1Y/3Y)

Terraform

Support

Custom

Image

Support

DigitalOcean

Memory

Intel

Optimized

Xeon

16

128 NVMe

0.03

N/A

N/A

Yes

Yes

Table 3: Provisioning & Automation Support

Provider

Terraform

Ansible

Support

Support

API

Custom

Documentation

Image

Quality

Support

Boot

Time

Notes

AWS

Yes

Yes

Excellent

Yes

Fast

GCP

Yes

Yes

Excellent

Yes

Fast

Azure

Yes

Yes

Excellent

Yes

Fast

CoreWeave Yes

Limited

Good

Yes

Moderate

RunPod

Yes

No

Good

Yes

Moderate

Linode

Yes

No

Good

Yes

Moderate

DigitalOcean Yes

No

Good

Yes

Moderate

Hetzner

No

No

Limited

No

Slow

OVHcloud

Yes

No

Moderate

Yes

Moderate

Full API, built-in Ansible

plugins

Full API, built-in Ansible

plugins

Full API, built-in Ansible

plugins

Kubernetes-native,

requires expertise

Integrates with SkyPilot,

Terraform

REST API, Terraform

modules

Simple API, Terraform

modules

Limited automation

support

OpenStack/Kubernetes

based

Table 4: Networking & Costs

Provider

Inter-Node

Bandwidth

Latency

Data Egress

Free Tier/

Cost

Egress

Notes

AWS

10 Gbps+

Low

Moderate

Limited

4/10

Provider

Inter-Node

Bandwidth

Latency

Data Egress

Free Tier/

Cost

Egress

Notes

Ultra-low-latency fiber, Direct

Connect

GCP

200 Gbps

Low

Moderate

Limited

Global fiber network, Cloud Router

Azure

100 Gbps

(InfiniBand)

Ultra-low Moderate

Limited

CoreWeave InfiniBand

Ultra-low Low

RunPod

Moderate

Moderate Low

No

No

ExpressRoute, InfiniBand

networking

High-bandwidth cluster

interconnects

Multi-region deployment

Linode

Moderate

Moderate Moderate

No

Limited global footprint

DigitalOcean Moderate

Moderate Moderate

No

Limited global footprint

Hetzner

Moderate

Moderate Moderate

No

OVHcloud Moderate

Moderate Moderate

No

European focus, limited global

reach

European focus, limited global

reach

Provider Deep Dives

AWS

AWS is the market leader with the broadest GPU instance portfolio and global infrastructure. It

offers NVIDIA A100, L40S, and other GPUs with high RAM and NVMe storage options. AWS’s

spot instances provide up to 90% discounts but suffer from frequent capacity shortages,

especially for high-demand GPU types. AWS supports Terraform and Ansible extensively,

enabling automated provisioning and configuration. Its bare-metal and confidential computing

options enhance security but add complexity. AWS’s network is highly redundant with ultra-

low-latency interconnects, ensuring high availability and performance. However, AWS’s

complex pricing and hidden costs can lead to budget overruns.

GCP

GCP offers NVIDIA H100 and other GPUs with up to 91% spot discounts and variable pricing.

Its accelerator-optimized instances are designed for AI and HPC workloads, with high

memory-to-vCPU ratios. GCP supports Terraform and Ansible well, enabling automated
deployments. GCP’s global fiber network ensures low-latency and high-bandwidth

connectivity, ideal for distributed proving workloads. However, GCP’s spot instance reliability

varies, and its pricing can be unpredictable.

5/10

Azure

Azure provides NVIDIA H100 and other GPU instances with up to 90% spot discounts. Its NC

and ND series VMs are optimized for AI and HPC workloads with InfiniBand networking for low

latency. Azure supports Terraform and Ansible, enabling automated provisioning. Azure’s

network is designed for high-speed, low-latency connections, suitable for enterprise

workloads. However, Azure’s spot instances can be interrupted on short notice, and its pricing

is complex.

CoreWeave

CoreWeave specializes in HPC and AI workloads, offering bare-metal GPU instances with

InfiniBand interconnects and Kubernetes-native orchestration. It provides up to 60% reserved

instance discounts and supports large multi-GPU clusters for distributed training. CoreWeave’s

infrastructure is optimized for zk-proving workloads requiring high bandwidth and low latency.

CoreWeave requires Kubernetes expertise and is ideal for organizations training foundation

models.

RunPod

RunPod offers containerized GPU environments with root access and transparent pricing

starting at $0.34/GPU-hour. It supports Terraform and integrates with SkyPilot, enabling rapid

prototyping and distributed training. RunPod’s spot market and community cloud provide cost-

effective access but may face capacity constraints. RunPod is developer-friendly and suitable

for fine-tuning large language models and distributed training.

Lambda Labs

Lambda Labs provides on-demand H100 GPUs at ~$2.99/GPU-hour, targeting AI researchers

and enterprises needing high-end hardware. Its hybrid cloud and colocation services allow

scaling compute resources without losing control or performance. Lambda Labs is ideal for

training large models and hybrid cloud deployments but lacks spot/preemptible instances.

Vast.ai

Vast.ai operates a competitive bidding marketplace for GPUs, offering prices starting around

$0.90/hour for H100. This enables cost savings but with variable host reliability and no uptime

guarantees. Vast.ai is suitable for experimentation and cost-sensitive work but not for

production workloads requiring guaranteed uptime.

Hetzner

Hetzner offers dedicated GPU servers in Europe starting at €3.49/month, with no spot/

preemptible instances and limited GPU availability. Its low cost is attractive but lacks flexibility

and global coverage. Hetzner’s network spans 17 locations and 37 data centers, serving over
1.6 million customers in 140 countries.

6/10

OVHcloud

OVHcloud specializes in European cloud services, offering GPU instances optimized for AI and

HPC workloads. Its network spans 17 locations and 37 data centers, serving over 1.6 million

customers in 140 countries. OVHcloud’s hardware is outdated in some regions, and its support

is subpar compared to hyperscalers.

Linode

Linode offers GPU instances starting at $0.0075/hour, designed for AI inference, big data

processing, and video encoding. Linode supports Terraform and custom images but lacks

spot/preemptible instances and has a smaller global footprint. Linode’s GPU instances are

optimized for high-throughput, low-latency inference at production scale.

DigitalOcean

DigitalOcean offers GPU Droplets starting at $0.76/GPU-hour, designed for high-performance

computing (HPC) workloads. DigitalOcean’s GPU Droplets are equipped with NVIDIA Blackwell
Ultra GPUs, delivering significantly expanded AI compute, memory capacity, and networking

bandwidth. DigitalOcean supports Terraform and custom images but lacks spot/preemptible

instances and has a smaller global footprint.

Recommendations

Cost-Optimized Picks

-

Vast.ai: Best for experimentation and cost-sensitive workloads due to its competitive

bidding marketplace offering the lowest GPU prices (~$0.90/hr for H100). However,

reliability varies by host, and it is not suitable for production workloads requiring

guaranteed uptime.

-

RunPod: Offers transparent pricing starting at $0.34/GPU-hour and integrates with

SkyPilot and Terraform, making it suitable for developers needing cost-effective GPU

access with automation support.

-

DigitalOcean: Provides straightforward pricing and GPU droplets starting at $0.76/GPU-

hour, ideal for developers valuing ease-of-use and predictable costs.

Performance Picks

-

CoreWeave: Best for high-performance, low-latency, and distributed proving workloads

requiring bare-metal GPU instances with InfiniBand interconnects and Kubernetes

orchestration. CoreWeave’s infrastructure is optimized for large-scale AI and ML
workloads.

-

AWS, GCP, Azure: Offer the broadest GPU instance portfolios, global data center

coverage, and robust automation support. Their high-bandwidth, low-latency networks

ensure high availability and performance for demanding workloads.

7/10

Automation-Friendly Picks

-

AWS, GCP, Azure: Support Terraform and Ansible extensively, enabling automated

provisioning, configuration, and orchestration. Their comprehensive APIs and built-in

plugins facilitate seamless integration with existing workflows.

-

Linode, DigitalOcean: Offer REST APIs and Terraform modules, suitable for developers

needing automation but with smaller global footprints and limited spot capacity.

Avoid Unless Necessary

-

Hetzner, OVHcloud: While cost-effective, they lack spot/preemptible instances and have

limited global coverage and outdated hardware in some regions. Their support and

managed services are subpar compared to hyperscalers.

Sources & Links

-

-

-

-

-

-

-

-

-

-

-

-

AWS Pricing & Docs: 1 2 3 4 5 6 7 8 9 10 11 12 13
GCP Pricing & Docs: 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29
Azure Pricing & Docs: 30 31 32 33 34 35 36 37 38 39 40 41
CoreWeave: 42 43
RunPod: 42 44
Lambda Labs: 42 44 19
Vast.ai: 45 44 19
Hetzner: 46 47 48 49 50 51 52 53 54 55
OVHcloud: 56 57 58
Linode: 59 60 61 62 63 64 65 66 67 68 69
DigitalOcean: 70 71 72 73 74 75 76 77 78 79 80 69
Terraform/Ansible: 81 82 83 84

This comprehensive market analysis provides a detailed comparison of cloud hosting

providers for zk-proof proving projects, focusing on GPU and high-memory CPU instances,

pricing, automation support, networking, and geographic availability. The structured tables and

deep dives enable informed decision-making based on project-specific requirements for

performance, cost efficiency, and flexibility.

[1] Amazon EC2 Pricing

[2] EC2 On-Demand Instance Pricing

[3] Amazon EC2 Spot Instances Pricing

[4] GPU Instance Pricing - AWS EC2 GPU Instances for AI/ML | DoiT Compute

[5] Selecting Ideal EC2 Instances for GPU Workloads on AWS

[6] AWS GPU Instance Pricing: P5, P4d, G5, Inf2 | Wring Blog

[7] AWS GPU Pricing Explained: Costs & Optimization Guide | TRG Datacenters

8/10

[8] Accelerated computing Amazon EC2 instance types

[9] Amazon EC2 G6 Instances | Amazon Web Services

[10] Amazon EC2 G4 Instances — Amazon Web Services (AWS)

[11] Amazon EC2 G5 Instances | Amazon Web Services

[12] Virtual machine sizes overview - Azure Virtual Machines | Azure Docs

[13] Azure VM Sizes & Pricing: A 2025 Guide for Engineering Teams | Sedai

[14] About GPU instances | Compute Engine | Google Cloud Documentation

[15] VM instance pricing - Compute Engine

[16] GPU pricing | Google Cloud

[17] Spot VMs | Compute Engine | Google Cloud Documentation

[18] r/MachineLearning on Reddit: [D] Training on the cloud: GCP GPU pricing seems

dramatically cheaper, why would you train on AWS or Azure?

[19] H100 Rental Prices Compared: $1.49-$6.98/hr Across 15+ Cloud Providers (2026) |

IntuitionLabs

[20] Demystifying Google Cloud GPU Pricing: What You Need to Know - Oreate AI Blog

[21] Pricing | Spot VMs | Google Cloud

[22] GPU machine types | Compute Engine | Google Cloud Documentation

[23] Accelerator-optimized machine family | Compute Engine | Google Cloud Documentation
[24] Demystifying Google Cloud Platform Compute Engine Machine Types: A Comprehensive

Guide | by Ayushmaan Srivastav | Medium

[25] GCP Instance Types: Summary, Comparison & Recommendations

[26] Machine families resource and comparison guide | Compute Engine | Google Cloud

Documentation

[27] Supported Cloud Instances | Clarifai Docs

[28] GCP Instance Types Explained: Making the Right Choice for Your Workloads |

CloudKeeper

[29] Navigating GCP Instance Types: What To Use And When

[30] Cloud GPU Pricing Comparison: AWS Vs Azure Vs GCP For AI Workloads (2026)

[31] Azure Virtual Machine Pricing - GeeksforGeeks

[32] Cloud GPU Pricing Comparison in 2025 — Blog

[33] Microsoft Azure GPU Pricing 2026 – H100 & A100 Costs

[34] Spot Virtual Machines – Spot Pricing and Features | Microsoft Azure

[35] Azure cloud GPU server costs 2023 | Statista

[36] Azure Pricing: The Complete Guide

[37] Virtual machine sizes overview - Azure Virtual Machines | Microsoft Learn

[38] Virtual Machine series | Microsoft Azure

[39] Microsoft Azure Instance Types: Comprehensive Comparison

[40] NV family VM size series - Azure Virtual Machines | Microsoft Learn

[41] Microsoft Azure Instance Types: Detailed Explanation

[42] Top 12 Cloud GPU Providers for AI and Machine Learning in 2026

[43] H100 Rental Prices: A Cloud Cost Comparison (Nov 2025)

[44] Cloud GPU Pricing Tracker — Compare AWS, Azure, GCP, CoreWeave, Lambda | Silicon
Analysts

[45] Cloud GPU Hosting Explained: Providers, Pricing, and Best …

[46] Hetzner Cloud for AI Projects — Complete GPU Server Setup & Cost Breakdown 2026 -

DEV Community

9/10

[47] Hetzner Cloud for AI Projects Complete GPU Server Setup & Cost Breakdown 2026 - by

Dev To

[48] Hetzner Cloud VPS Pricing Calculator (Apr 2026)

[49] Hetzner presents a GPU server for trained AI models

[50] GEX44

[51] Hetzner | Review, Pricing & Alternatives

[52] Cloud-hosting provider for developers & teams - Hetzner

[53] High performance dedicated GPU server - Hetzner GEX131

[54] Hetzner GPU Cloud | Compare & Launch with Shadeform

[55] Hetzner GPU Server | Hacker News

[56] How to Choose a Cloud GPU Provider for AI/ML Workloads in 2026 | DigitalOcean

[57] Top 10 Microsoft Azure Alternatives for Cloud Apps in 2026 | DigitalOcean

[58] Top 10 Cloud Service Providers in 2026 - Utho

[59] Linode (Akamai) VPS: 75 Plans From $5.47/mo — Simple Cloud Hosting (2026)

[60] Cloud Computing Pricing | Akamai - Linode

[61] How does the GPU billing works? (19777) | Linode Questions

[62] GPU Plans by Linode | VPSBenchmarks

[63] Linode and NVIDIA Cloud GPU Instances | Akamai
[64] Linode GPU instances offer accelerated computing power | TechTarget

[65] Linode VM Pricing | Compare Instance Prices - Holori Calculator

[66] Choose a compute plan

[67] Linode Pricing - Actual Prices For All Plans, Enterprise Too

[68] GPU Linodes

[69] Top 10 Linode Alternatives for Cloud Computing in 2026 | DigitalOcean

[70] GPU Droplets Pricing | DigitalOcean

[71] Budget-Friendly Cloud Server Pricing | DigitalOcean

[72] Droplet Pricing | DigitalOcean Documentation

[73] Gradient™ AI GPU Droplets | DigitalOcean

[74] 7 Best Cloud GPU Platforms for AI, ML, and HPC in 2025 | DigitalOcean

[75] DigitalOcean Pricing: VPS, GPU, Storage and More (2026)

[76] r/digital_ocean on Reddit: Rent by the hour GPU option?

[77] 7 Strategies for Effective GPU Cost Optimization | DigitalOcean

[78] Pricing | DigitalOcean

[79] DigitalOcean GPU Droplets, scalable computing power on demand | DigitalOcean

[80] DigitalOcean Servers Lineup at Cloudways | Cloudways Help Center

[81] Ansible vs. Terraform

[82] Terraform & Ansible: Unifying infrastructure provisioning and configuration management

[83] Integrate Terraform with Ansible Automation Platform | HashiCorp Developer

[84] Terraform + Ansible: When to Use Each and How to Integrate Them

10/10
