# Requirements to effectively use cloud services

## Components

- Queue (redis)
    - Holds tasks of proofs we need
    - Persisted to local disk
- RDBMS
    - Sqlite (?); could write soley from the worker on the box
    - Holds
        - Records of proof generations
        - Headers
        - Current status through the historical generation
- Object storage
    - Holds the generated proofs
    - Must be able to be accessed by the Worker
- Proof Worker Image
    - Must handle each proof type
    - Must be able to be spun up as a single Worker
    - Must automatically connect to redis and start looking for tasks
- S3 Saver Worker Image
    - Watches a queue for completed proofs and saves them to object storage
    - Also updates the RDBMS with the status of the proof generation
    - Move the task from "s3-saver:in-progress" to "done"

## Resources

- 1 cheap VPS
    - Redis
    - Postgres
    - Image registry
    - Tailscale
- 1 S3 bucket
    - /proofs/shards, /proofs/compressed, /proofs/metadatam
    - Hold docker images between runs
- Grafana account
-

## Tooling

- Ansible Play to configure fresh machines with the necessary software and dependencies
- Ansible Play to create a new machine
- Ansible Play to create tear down a machine
- A facility inside the proof workers to monitor a side-thread with a worker that queries the instance metadata
  frequently watching for preemption
    - When detected the proof worker should stop taking new work.
    -
- Docker on the machine to run the workers
-

## Configs

- SP1_PROVER=cuda
- docker-compose.yml
    - Initially oriented toward having 1 proof worker at a time
    - Has:
        - Proof worker
        - S3 saver worker
        - Exportersgg
            - app
            - server?
