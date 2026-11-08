## [0.2.0](https://github.com/brinkercode/freewheel/compare/v0.1.0...v0.2.0) (2026-11-20)


### ⚠ BREAKING CHANGES

* **config:** mappings files from 0.0.x no longer load; export them again from 0.1.0

### New

* **device:** Speed Editor key and wheel LEDs driven from the engine ([#141](https://github.com/brinkercode/freewheel/issues/141)) ([4025bcd](https://github.com/brinkercode/freewheel/commit/4025bcdf254e648917541da5428e7c5bb5918652))
* **e2e:** the Windows e2e clicks every control ([#138](https://github.com/brinkercode/freewheel/issues/138)) ([1b2c3d4](https://github.com/brinkercode/freewheel/commit/1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e))
* **freewheel-ab1:** Start at login ([#99](https://github.com/brinkercode/freewheel/issues/99)) ([4bb4735](https://github.com/brinkercode/freewheel/commit/4bb4735a2a6f895acf3d4a10021ace24368d1791))
* **updates:** portable releases and a portable self-update ([#145](https://github.com/brinkercode/freewheel/issues/145)) ([6895323](https://github.com/brinkercode/freewheel/commit/68953235d8a1c2b4e6f7a8b9c0d1e2f3a4b5c6d7))
* the wheel can step the volume, with a `Shift` held
  for finer steps ([7a8b9c0](https://github.com/brinkercode/freewheel/commit/7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b))


### Fixed

* **ci:** survive hung apt installs ([#120](https://github.com/brinkercode/freewheel/issues/120)) ([aa11bb2](https://github.com/brinkercode/freewheel/commit/aa11bb22cc33dd44ee55ff66aa77bb88cc99dd00))
* **deps-dev:** bump vitest to 4.0.1 ([#150](https://github.com/brinkercode/freewheel/issues/150)) ([bb22cc3](https://github.com/brinkercode/freewheel/commit/bb22cc33dd44ee55ff66aa77bb88cc99dd00ee11))
* **device:** real Speed Editor auth response ([#140](https://github.com/brinkercode/freewheel/issues/140)) ([cc33dd4](https://github.com/brinkercode/freewheel/commit/cc33dd44ee55ff66aa77bb88cc99dd00ee11ff22))
* **footprint:** tighter RAM budget ([dd44ee5](https://github.com/brinkercode/freewheel/commit/dd44ee55ff66aa77bb88cc99dd00ee11ff22aa33))
* **release:** sign the macOS zip too ([ee55ff6](https://github.com/brinkercode/freewheel/commit/ee55ff66aa77bb88cc99dd00ee11ff22aa33bb44))
* **scripts:** resolve the three CodeQL alerts ([#148](https://github.com/brinkercode/freewheel/issues/148)) ([a97b649](https://github.com/brinkercode/freewheel/commit/a97b649a1b2c3d4e5f60718293a4b5c6d7e8f90a))
* **test:** the flaky tray test waits for the menu ([ff66aa7](https://github.com/brinkercode/freewheel/commit/ff66aa77bb88cc99dd00ee11ff22aa33bb44cc55))
* **workflow:** pin setup-node ([#149](https://github.com/brinkercode/freewheel/issues/149)) ([70e136a](https://github.com/brinkercode/freewheel/commit/70e136a1b2c3d4e5f60718293a4b5c6d7e8f90ab))
* header status never overlaps; tray and window share one On/Off ([#146](https://github.com/brinkercode/freewheel/issues/146)) ([ae3cfe4](https://github.com/brinkercode/freewheel/commit/ae3cfe41b2c3d4e5f60718293a4b5c6d7e8f90ab))


### Faster

* **footprint:** measure the exe size ([0a1b2c3](https://github.com/brinkercode/freewheel/commit/0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d))


### Reverts

* 6902b46448c1 failed the merge audit ([#77](https://github.com/brinkercode/freewheel/issues/77)) ([63ad24e](https://github.com/brinkercode/freewheel/commit/63ad24e2f94546399cc7ad3a0c798a3bb8cfcc64))
