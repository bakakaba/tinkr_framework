# Changelog

## [0.1.11](https://github.com/bakakaba/tinkr_framework/compare/v0.1.10...v0.1.11) (2026-08-13)


### Features

* **config:** support explicit and $CONFIG_FILE config file paths ([61980dd](https://github.com/bakakaba/tinkr_framework/commit/61980dd4386fd7492ee8cebeb251d1ab24045a47))

## [0.1.10](https://github.com/bakakaba/tinkr_framework/compare/v0.1.9...v0.1.10) (2026-08-06)


### Features

* semconv HTTP request metrics with gRPC-aware status ([b17f884](https://github.com/bakakaba/tinkr_framework/commit/b17f884169872c9f08e44c087b00f355edf6b7a6))

## [0.1.9](https://github.com/bakakaba/tinkr_framework/compare/v0.1.8...v0.1.9) (2026-08-04)


### Features

* auth headers and TLS/mTLS for OTLP export ([9d7fa8e](https://github.com/bakakaba/tinkr_framework/commit/9d7fa8e408449bb19b2ed25b35ce54ed0a4a6ebb))

## [0.1.8](https://github.com/bakakaba/tinkr_framework/compare/v0.1.7...v0.1.8) (2026-08-03)


### ⚠ BREAKING CHANGES

* the `gcp` feature is removed; deployed logs always use the new JSON format, which Cloud Logging ingests natively.

### Features

* OpenTelemetry export over OTLP (traces, metrics, logs) ([#16](https://github.com/bakakaba/tinkr_framework/issues/16)) ([7cd20f0](https://github.com/bakakaba/tinkr_framework/commit/7cd20f0759795206174d032362ff02b7e36054cc))

## [0.1.7](https://github.com/bakakaba/tinkr_framework/compare/v0.1.6...v0.1.7) (2026-07-27)


### Features

* gRPC server reflection ([#14](https://github.com/bakakaba/tinkr_framework/issues/14)) ([f7edb52](https://github.com/bakakaba/tinkr_framework/commit/f7edb528e93dbcae8f43424115b0d375257ccf21))

## [0.1.6](https://github.com/bakakaba/tinkr_framework/compare/v0.1.5...v0.1.6) (2026-07-23)


### Features

* add name and version to startup log ([bde9df7](https://github.com/bakakaba/tinkr_framework/commit/bde9df7aaa4d7630e1ed9fce4823b5c6a1d4dc6e))

## [0.1.5](https://github.com/bakakaba/tinkr_framework/compare/v0.1.4...v0.1.5) (2026-07-21)


### Bug Fixes

* revert tonic reexports as there is no benefit to it ([8900c2a](https://github.com/bakakaba/tinkr_framework/commit/8900c2a412c40947e362d491fd60484e31923464))

## [0.1.4](https://github.com/bakakaba/tinkr_framework/compare/v0.1.3...v0.1.4) (2026-07-20)


### Bug Fixes

* fix release missing id-token ([06ea32c](https://github.com/bakakaba/tinkr_framework/commit/06ea32cabc9e14a89b0bd8a6c525eaffba4aa7fe))
* fix versioning to be locked to single version for all packages ([f402eb4](https://github.com/bakakaba/tinkr_framework/commit/f402eb4d4604e560ba25a2983f2ceb84479c1816))

## [0.1.3](https://github.com/bakakaba/tinkr_framework/compare/v0.1.2...v0.1.3) (2026-07-20)


### Features

* re-export tonic-prost and prost so consumer gRPC deps stay in lockstep ([692d3b2](https://github.com/bakakaba/tinkr_framework/commit/692d3b252d9117740db82917e4ecc0515cec3bae))

## [0.1.2](https://github.com/bakakaba/tinkr_framework/compare/v0.1.1...v0.1.2) (2026-07-17)


### Features

* add built-in health reporting ([408f42f](https://github.com/bakakaba/tinkr_framework/commit/408f42ff1e7b53880d5342e4d127297a4521302a))
* add configuration to the framework ([05df1b3](https://github.com/bakakaba/tinkr_framework/commit/05df1b3c381ae0fd68b374e819ba10588c5bf6df))
* default the RUST_LOG to info ([2f167cf](https://github.com/bakakaba/tinkr_framework/commit/2f167cfc9264a0fabfa5a13e3ff326ace638dee8))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * tinkr_config bumped from 0.1.1 to 0.1.2

## [0.1.1](https://github.com/bakakaba/tinkr_framework/compare/v0.1.0...v0.1.1) (2026-07-13)


### Bug Fixes

* add missing link to docs ([b341dc8](https://github.com/bakakaba/tinkr_framework/commit/b341dc83049e80adad3fc45eed6d70819308321a))

## 0.1.0 (2026-07-13)


### Features

* add bootstrap ([e82fc83](https://github.com/bakakaba/tinkr_framework/commit/e82fc83c28ce0e452cf8b4bf82ae4e4678593b5f))
* initial server implementation ([8d3b079](https://github.com/bakakaba/tinkr_framework/commit/8d3b0790e7f85c1708f09d97a5d5edde172cfacc))
