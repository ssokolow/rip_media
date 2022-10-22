**WARNING:** This is my _very first_ Rust codebase and sat mouldering since
2016, modulo some superficial clean-ups and migrations like error-chain to
anyhow. It was never fully completed and is _badly_ in need of refactoring.

---

The beginnings of a wrapper for automating all of the steps I go through to make
verified,
[FEC](https://en.wikipedia.org/wiki/Forward_error_correction)-augmented backups
of my CDs/DVDs/cartridges/etc.

Currently unusable because I still have a little bit of critical glue code
remaining to be ported over from the Python-based precursor, but much of the
supporting code _has_ been completed and has 100% unit test coverage.
