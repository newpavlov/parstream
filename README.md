# parstream
Compute function over iterator in a streaming fashion using thread pool and
preserving order of elements.

## Example
```rust
let xs: &[u64] = &[100, 4, 3, 2, 1, 0, 1, 2, 3, 4, 5];
let mut ys = Vec::new();
let f = |x| x*x;
parstream::run(xs, 4,
    |x| {
        std::thread::sleep(std::time::Duration::from_millis(*x));
        f(x)
    },
    |y| ys.push(y),
);
assert_eq!(ys, xs.iter().map(f).collect::<Vec<_>>());
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
