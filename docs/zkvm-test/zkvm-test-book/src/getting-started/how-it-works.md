# How the runner works

*This page is an outline. Planned contents:*

- What the cargo plugin compiles and where it puts artifacts
- The two phases: listing the tests in an ELF, then running each one in its own VM instance
- Why per-test isolation makes a guest panic a test failure rather than a suite abort
- Where cycle counts and trace sizes come from
- How the same source compiles for both the host and the zkVM
