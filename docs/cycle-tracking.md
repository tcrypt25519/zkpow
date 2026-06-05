# Cycle Tracking | Succinct Docs
Version: v6 (Hypercube)

When writing a program, it is useful to know how many RISC-V cycles a portion of the program takes to identify potential performance bottlenecks. SP1 provides a way to track the number of cycles spent in a portion of the program.

To enable cycle tracking, use the `profiling` feature for `sp1-sdk`:

```
sp1-sdk = { version = "6.0.0", features = ["profiling"] }

```


Tracking Cycles
---------------------------------------------------------------------

### Using Print Annotations

For simple debugging, use these annotations to log cycle counts to stdout:

```
#![no_main]
sp1_zkvm::entrypoint!(main);

fn main() {
     let mut nums = vec![1, 1];

     // Compute the sum of the numbers.
     println!("cycle-tracker-start: compute");
     let sum: u64 = nums.iter().sum();
     println!("cycle-tracker-end: compute");
}

```


With this code, you will see output like the following in your logs:

```
[INFO] compute: 1234 cycles

```


### Using Report Annotations

To store cycle counts across multiple invocations in the `ExecutionReport`, use the report annotations:

```
#![no_main]
sp1_zkvm::entrypoint!(main);

fn main() {
    // Track cycles across multiple computations
    for i in 0..10 {
        println!("cycle-tracker-report-start: compute");
        expensive_computation(i);
        println!("cycle-tracker-report-end: compute");
    }
}
```

Access total cycles from all invocations:

```
let report = client.execute(ELF, &stdin).run().unwrap();
let total_compute_cycles = report.cycle_tracker.get("compute").unwrap();

```

note

The `cycle_tracker` and `invocation_tracker` fields in the `ExecutionReport` are only populated when the `profiling` feature is enabled for `sp1-sdk`. Without it, these fields will be empty.

Access the number of invocations for `cycle-tracker-report-*` in the program:

```
let compute_invocation_count = report.invocation_tracker.get("compute").unwrap();

```


### Using the Cycle Tracker Macro

Add `sp1-derive` to your dependencies:

Then annotate your functions:

```
#[sp1_derive::cycle_tracker]
pub fn expensive_function(x: usize) -> usize {
    let mut y = 1;
    for _ in 0..100 {
        y *= x;
        y %= 7919;
    }
    y
}

```
