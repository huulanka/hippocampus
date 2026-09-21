# [1.2.0](https://github.com/huulanka/hippocampus/compare/v1.1.0...v1.2.0) (2026-09-21)


### Features

* replace ort with candle for local inference ([#33](https://github.com/huulanka/hippocampus/issues/33)) ([171ec1f](https://github.com/huulanka/hippocampus/commit/171ec1f7c6acba2c18e43ceef23a28cedb35759a))

# [1.1.0](https://github.com/huulanka/hippocampus/compare/v1.0.0...v1.1.0) (2026-09-21)


### Bug Fixes

* make the NAS data directory configurable ([#32](https://github.com/huulanka/hippocampus/issues/32)) ([fd61736](https://github.com/huulanka/hippocampus/commit/fd61736a3fe7bc3488afc82ee381208f0b9cd7d8))


### Features

* client sends Cloudflare Access Service Token headers ([#31](https://github.com/huulanka/hippocampus/issues/31)) ([5dc2c83](https://github.com/huulanka/hippocampus/commit/5dc2c8377cd3b3f4eb9f8ce34c58cbcd840a6117))

# 1.0.0 (2026-09-21)


### Bug Fixes

* **client:** the microphone was never asked for ([#18](https://github.com/huulanka/hippocampus/issues/18)) ([13ed20c](https://github.com/huulanka/hippocampus/commit/13ed20c55d350b16e6564eb4e4cc8643ed265204))


### Features

* "morgen" now means a date ([#24](https://github.com/huulanka/hippocampus/issues/24)) ([87bce0d](https://github.com/huulanka/hippocampus/commit/87bce0d4887b59e9153d132c8fcf099d18a9410d))
* a capture can be corrected without losing what it said ([#22](https://github.com/huulanka/hippocampus/issues/22)) ([bcd3eaa](https://github.com/huulanka/hippocampus/commit/bcd3eaa7a506fb84993005c6ec708e3824169e27))
* a surface that speaks first ([#26](https://github.com/huulanka/hippocampus/issues/26)) ([e6a08a1](https://github.com/huulanka/hippocampus/commit/e6a08a109b4eae46470931a500b45d667f4a5083))
* backend Docker image for NAS deployment ([#30](https://github.com/huulanka/hippocampus/issues/30)) ([4267dd7](https://github.com/huulanka/hippocampus/commit/4267dd7483366e956a68ec962411b229ad304c54))
* **backend:** close the front door ([#15](https://github.com/huulanka/hippocampus/issues/15)) ([ca029c9](https://github.com/huulanka/hippocampus/commit/ca029c9ce980c12aa06104524a3a95d736eb0f17))
* **backend:** echo endpoint, RRF search, time and entity filters ([#9](https://github.com/huulanka/hippocampus/issues/9)) ([d3a6c8b](https://github.com/huulanka/hippocampus/commit/d3a6c8baabe079cd8bffff7e8be646714cb36cb5))
* **backend:** store audio as the original, transcript as interpretation ([#12](https://github.com/huulanka/hippocampus/issues/12)) ([96b71c8](https://github.com/huulanka/hippocampus/commit/96b71c8aed9b6d2f203fe4055ccd2384f559cc79))
* **client:** global shortcut summons the capture field ([#11](https://github.com/huulanka/hippocampus/issues/11)) ([d273c10](https://github.com/huulanka/hippocampus/commit/d273c10d5b6709d9d99373c96116f6bd533f6974))
* **client:** real capture with inline echo, working search filters ([#10](https://github.com/huulanka/hippocampus/issues/10)) ([e864f5e](https://github.com/huulanka/hippocampus/commit/e864f5ebed6046855bc3ac418beacf263c6d2fce))
* **client:** speak a capture, transcribed on this Mac ([#13](https://github.com/huulanka/hippocampus/issues/13)) ([ca71483](https://github.com/huulanka/hippocampus/commit/ca714837e4fa435bbaa0b1e12ecfc7b08fdc478c))
* **client:** the capture shortcut is yours to choose ([#14](https://github.com/huulanka/hippocampus/issues/14)) ([12aadd7](https://github.com/huulanka/hippocampus/commit/12aadd7b46e928277b8ae9a32602e9fbc8add413))
* configurable backend URL, real logging, version/About, real app icon ([#28](https://github.com/huulanka/hippocampus/issues/28)) ([fed3fde](https://github.com/huulanka/hippocampus/commit/fed3fded15284f4855ed03af007a34fc6e754f69))
* echo is judged by a model that reads both texts ([#25](https://github.com/huulanka/hippocampus/issues/25)) ([584cf3f](https://github.com/huulanka/hippocampus/commit/584cf3f093873564878ad043555ba49ef95b064e))
* entities are a place you can go ([#21](https://github.com/huulanka/hippocampus/issues/21)) ([73cfce8](https://github.com/huulanka/hippocampus/commit/73cfce8989d04298279adab6012271f038160a9e))
* every capture can be opened and inspected ([#19](https://github.com/huulanka/hippocampus/issues/19)) ([1482629](https://github.com/huulanka/hippocampus/commit/14826292a97f4505e80e4e6fb1a38888f70bcfa9))
* implement Hippocampus UI from design mockup ([#7](https://github.com/huulanka/hippocampus/issues/7)) ([92b8bf3](https://github.com/huulanka/hippocampus/commit/92b8bf324501ac59d57976d86f776871a973bdfb))
* local semantic embeddings + hybrid search over captures ([#5](https://github.com/huulanka/hippocampus/issues/5)) ([bff8488](https://github.com/huulanka/hippocampus/commit/bff8488739be102462c4eba4537aa37c65680436))
* scaffold Hippocampus workspace with working ingest pipeline ([6d971bd](https://github.com/huulanka/hippocampus/commit/6d971bd615134ca58fab87f024fa6b39bbea08b3))
* structure captures via OpenRouter into entities/relations ([#6](https://github.com/huulanka/hippocampus/issues/6)) ([43129c6](https://github.com/huulanka/hippocampus/commit/43129c69f9bb269913c3ad3e1fec1602be134283))
