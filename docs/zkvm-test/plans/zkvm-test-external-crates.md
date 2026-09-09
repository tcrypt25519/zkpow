# Creates We Should Check Out

## Primativew

- Bitflag, enumflags, or similar: Something take over the existing bitflag setup so it's not a bunch of independant
  variables, so they can easily be tied to their string-arg counterparts, adding matching semantics, etc

## Errors

color-eyre

## CLI

There are some CLI features we're missing but idk that Clap or similar is ideal either.

- I would like to have a more automated help menu so it does get out of date
- Output/rendering helpers would be nice for the instruction table and cycle counts
- libtest-mimic for a more cohesive feel with test/nextest?
- Need to use colors to be visually appealing
  - colorz (no deps) or owo-colors: Using color in our output
  - Or perhaps [libtest's pretty format](https://github.com/rust-lang/rust/blob/main/library/test/src/formatters/pretty.rs)
- pretty_assertions?

## Reports

junit-report, quick-junit, or [libtest's junit.rs](github.com/rust-lang/rust/blob/main/library/test/src/formatters/junit.rs): Do the junit work for us instead of using a hand-built version that we would have to iterate on for fixes and eventually

## Instrumentaton

We should come up with a way to monitor the runner itself while running.
