# Changelog

release-please writes this file from the Conventional Commits on `main`;
do not edit it by hand. Versions follow semver and match the `version` field
in `Cargo.toml` and `herdr-plugin.toml`.

## [1.1.1](https://github.com/wapenshaw/herdr-customize/compare/v1.1.0...v1.1.1) (2026-11-22)


### Bug Fixes

* **app:** keep the app's default keys before its first start ([c8af3a6](https://github.com/wapenshaw/herdr-customize/commit/c8af3a6d8d9eaa13503e92131ddccab26b717598))
* **app:** never replace a key bound during app install ([d5c6125](https://github.com/wapenshaw/herdr-customize/commit/d5c6125fd84bd345679ac55b6aebb5b27797d924))
* **app:** read the keys before writing the popup mod ([c61ce63](https://github.com/wapenshaw/herdr-customize/commit/c61ce6351b11de916d5751582d62f80743de0125))

## [1.1.0](https://github.com/wapenshaw/herdr-customize/compare/v1.0.0...v1.1.0) (2026-11-22)


### Features

* **app:** edit and update other machines' core settings ([efb396d](https://github.com/wapenshaw/herdr-customize/commit/efb396de97f0ff3b86777f2f3dd680ab30487944))
* **app:** list installed agents before the rest on the programs page ([966d6e5](https://github.com/wapenshaw/herdr-customize/commit/966d6e5f6585408a23388126c6ab91ec983af48b))
* **app:** pin status to a bar under every page ([b6c058c](https://github.com/wapenshaw/herdr-customize/commit/b6c058cb437cce042bf5fef69e0a0b7947c7b84c))
* **app:** show which agents each machine has on the programs page ([728629f](https://github.com/wapenshaw/herdr-customize/commit/728629fef002127ded2cff842b1941989a042053))


### Bug Fixes

* **app:** write settings only when save is pressed ([dedc7c2](https://github.com/wapenshaw/herdr-customize/commit/dedc7c2037fccf7f7de5e882d8f2d752b45be929))


### Refactoring

* **app:** drop an unread record field and needless copies ([16cc10c](https://github.com/wapenshaw/herdr-customize/commit/16cc10c9d51f4664b976b0c3814e252c8f13aaf1))
* **app:** fit the core updates card on one row ([8d27f47](https://github.com/wapenshaw/herdr-customize/commit/8d27f479f4f89c900607cb00e91593f87f45b499))
* **deps:** drop regex, libc and windows-sys ([7b8ca74](https://github.com/wapenshaw/herdr-customize/commit/7b8ca74fa9231c168b33fc3bb2cce82da2314b0c))

## [1.0.0](https://github.com/wapenshaw/herdr-customize/compare/v0.10.0...v1.0.0) (2026-11-21)


### ⚠ BREAKING CHANGES

* **tui:** herdr-plugin.toml is gone, so `herdr plugin install` and `herdr plugin update` no longer work on this repository's main branch, and the ui, open, bind, apply, reset, sync and preview commands are removed. v0.10.0 is the last release with the TUI plugin; pin it with `herdr plugin install wapenshaw/herdr-customize --ref v0.10.0`.

### Bug Fixes

* **app:** fit all eight page tabs in the customize header ([dd2212e](https://github.com/wapenshaw/herdr-customize/commit/dd2212ef8de833dd73aad986f59788a32c342b6a))
* **app:** show numbers the app wrote in single precision as typed ([50a3e0d](https://github.com/wapenshaw/herdr-customize/commit/50a3e0da682061f9cd1f0faeed36d8124e4df703))


### Refactoring

* **tui:** drop the herdr tui plugin ([9d92cc9](https://github.com/wapenshaw/herdr-customize/commit/9d92cc969e4d3e79eb94a19fae319b86dad84aea))

## [0.10.0](https://github.com/wapenshaw/herdr-customize/compare/v0.9.0...v0.10.0) (2026-11-21)


### Features

* **app:** add a layouts page to switch, clone and style layouts ([5c9df90](https://github.com/wapenshaw/herdr-customize/commit/5c9df90cb6472f065f6f15d25da243eeeb94cbeb))
* **app:** add a programs page for the app's programs.json ([2a48c86](https://github.com/wapenshaw/herdr-customize/commit/2a48c86db7b2577dff206a610f50802da836499e))
* **app:** add app settings, extras, health and keys form pages ([bfd8942](https://github.com/wapenshaw/herdr-customize/commit/bfd89422ffbb1ff847e32d79fae168b0b8834231))
* **app:** add copy mode and scrollback extras ([d13051a](https://github.com/wapenshaw/herdr-customize/commit/d13051ae57f77fab3d3ebe873014d535a8864b5c))
* **app:** add popups that open a command in a floating terminal ([44d66ab](https://github.com/wapenshaw/herdr-customize/commit/44d66ab235508461dc326caba7de603e651d1d12))
* **app:** explain popups and start the form from examples ([d47a1a6](https://github.com/wapenshaw/herdr-customize/commit/d47a1a612793863193a29d5012722fef164ca674))
* **app:** make the programs page click-to-customize ([47b11fa](https://github.com/wapenshaw/herdr-customize/commit/47b11fa7d672e2183e1e7dfa0e521e0262a68032))
* **app:** put layouts beside mods with a community gallery ([76eec56](https://github.com/wapenshaw/herdr-customize/commit/76eec56ae45227b70af7866e1d578cb6d319575c))
* **app:** run the monitor popup example in btop's theme colors ([ee2db38](https://github.com/wapenshaw/herdr-customize/commit/ee2db383dc905945916a3af6de02b9d894aaafac))
* **app:** show a loader with elapsed seconds while GitHub is read ([0177858](https://github.com/wapenshaw/herdr-customize/commit/01778583f445b68bacc544d487b3db1e337439ff))
* **app:** warn when a PowerShell shell drops the core's folder hook ([818ed02](https://github.com/wapenshaw/herdr-customize/commit/818ed02e9a488fb58ea8491b07c232c2e0fdd6e2))


### Bug Fixes

* **app:** close Customize before opening a popup from Try ([6a692ba](https://github.com/wapenshaw/herdr-customize/commit/6a692ba4296f05c748f144c73ebe8837f881fd9d))
* **app:** end a nonpersistent popup's terminal when its window goes ([8fa8915](https://github.com/wapenshaw/herdr-customize/commit/8fa8915f49601212f56b37ab5d6b631e96ad64be))
* **app:** fetch only a package's folders when reviewing a repository ([0cbeeb7](https://github.com/wapenshaw/herdr-customize/commit/0cbeeb7eb7f5e36ca4761a2dc568104f0a6a77ab))
* **app:** keep spaced shell arguments intact on the core page ([804e956](https://github.com/wapenshaw/herdr-customize/commit/804e9565a40206d8d5c1148d8f91ff0310db3fb8))
* **app:** open scrollback in the default text app on windows ([166f083](https://github.com/wapenshaw/herdr-customize/commit/166f083a603f734dee450f713a49a5ed53910438))
* **app:** pin the program form's buttons and refresh agents after a save ([cacc4b9](https://github.com/wapenshaw/herdr-customize/commit/cacc4b91bdb88487328f2a8a4c704b634723d90e))
* **app:** start one terminal per popup however quickly it is opened ([6cdbbb1](https://github.com/wapenshaw/herdr-customize/commit/6cdbbb19f72d76bf9cb42da568a8f749a93291bf))
* **proc:** return at a program's exit, not when its children close the pipes ([18f9f9e](https://github.com/wapenshaw/herdr-customize/commit/18f9f9e61344b4be66ab64761aaf5e4f1a9d98b7))


### Refactoring

* **app:** call the bundled extras bundled mods ([1d03df5](https://github.com/wapenshaw/herdr-customize/commit/1d03df571dc3c45023d57d6404ebe8580517201c))
* **app:** move bundled mods into a view of the Mods page ([63fcbff](https://github.com/wapenshaw/herdr-customize/commit/63fcbff937b22f3412294596c9dbb654d288a14c))

## [0.9.0](https://github.com/wapenshaw/herdr-customize/compare/v0.8.4...v0.9.0) (2026-11-06)


### Features

* **app:** set herdr-core's settings on a Core page ([2f2079c](https://github.com/wapenshaw/herdr-customize/commit/2f2079c9fac0951984c27f686ec435e32cf4de98))


### Bug Fixes

* **app:** list this screen among the installed mods ([5ceb8e7](https://github.com/wapenshaw/herdr-customize/commit/5ceb8e784fcec5872e55bcfa9110d795446bf50e))
* **app:** offer the keys a new app version ships ([4c53f20](https://github.com/wapenshaw/herdr-customize/commit/4c53f2043b0bc68059e79595378ffd59803c32ef))

## [0.8.4](https://github.com/wapenshaw/herdr-customize/compare/v0.8.3...v0.8.4) (2026-11-06)


### Bug Fixes

* **app:** give the screen's calls up to ten minutes ([1793eda](https://github.com/wapenshaw/herdr-customize/commit/1793edadcaab74cd0e6ae02f794a2580173007f3))
* **app:** list and keep key bindings that take arguments ([fb483f6](https://github.com/wapenshaw/herdr-customize/commit/fb483f67e6e09680a1e046467933197cff54bdd8))

## [0.8.3](https://github.com/wapenshaw/herdr-customize/compare/v0.8.2...v0.8.3) (2026-11-01)


### Bug Fixes

* **app:** list and review mods the way herdr.dev does ([c4e2605](https://github.com/wapenshaw/herdr-customize/commit/c4e2605a2aac33e9fe9acf3c13004355d0b93389))

## [0.8.2](https://github.com/wapenshaw/herdr-customize/compare/v0.8.1...v0.8.2) (2026-11-01)


### Bug Fixes

* **app:** review a pasted github link ([87bc227](https://github.com/wapenshaw/herdr-customize/commit/87bc2270ff7bb9f3f2b85375f843543a430edbf6))
* **app:** save a key on enter only when save would ([d445024](https://github.com/wapenshaw/herdr-customize/commit/d4450240e76f6d927935c76b4fc2f0c24fff719d))


### Performance

* **app:** start the program less from the screen ([b338f82](https://github.com/wapenshaw/herdr-customize/commit/b338f829af126955ebc084b047876c2cd090535c))


### Refactoring

* **app:** share the screen's repeated pieces ([02ec04a](https://github.com/wapenshaw/herdr-customize/commit/02ec04a957ff079d666a0ba4fe65cb9fb1bb9d3b))

## [0.8.1](https://github.com/wapenshaw/herdr-customize/compare/v0.8.0...v0.8.1) (2026-11-01)


### Bug Fixes

* **app:** open the screen with ctrl+shift+y ([d103f99](https://github.com/wapenshaw/herdr-customize/commit/d103f9917e154c17f4d1eb520e75689c78d19704))
* **app:** warn when a more specific section hides a key ([40bbe2f](https://github.com/wapenshaw/herdr-customize/commit/40bbe2ff676f715a144cd86f59c9c91253192281))

## [0.8.0](https://github.com/wapenshaw/herdr-customize/compare/v0.7.0...v0.8.0) (2026-11-01)


### Features

* **app:** color the app's panels from the palette ([697d793](https://github.com/wapenshaw/herdr-customize/commit/697d79319ed7d8c53f985ec73c18073fb18e0160))
* **app:** customize the herdr desktop app's themes, keys and mods ([ece3326](https://github.com/wapenshaw/herdr-customize/commit/ece3326d21ba62db643b53aeeebc0371bda15395))
* **app:** draw the app's screen with its own controls ([5c75e0b](https://github.com/wapenshaw/herdr-customize/commit/5c75e0b36674fb8b3e0b0b314bbaf95f345bbd77))
* **app:** find the binary when the screen was copied in ([ca43d78](https://github.com/wapenshaw/herdr-customize/commit/ca43d784927af974594b99f8b3701f52cc78f7d5))


### Bug Fixes

* **app:** size the screen to the window ([de8d51c](https://github.com/wapenshaw/herdr-customize/commit/de8d51c1658e410fbabd43a4f28ebd9232986745))
* **app:** switch the screen's pages with tab ([7e8ae72](https://github.com/wapenshaw/herdr-customize/commit/7e8ae722ec6d6d1ecbac3fea402a9a426be45e2b))


### Refactoring

* **app:** keep the app's screen as a mod package ([1fb0b05](https://github.com/wapenshaw/herdr-customize/commit/1fb0b050c0403d1074d73e2c5e00d6eb75fc40a0))
* **app:** pick themes in the app's own settings ([f792d0f](https://github.com/wapenshaw/herdr-customize/commit/f792d0f0c38dba8e11290418e6521d35562ddcf5))
* **plugins:** search the marketplace by topic ([e95252c](https://github.com/wapenshaw/herdr-customize/commit/e95252cd2252440b4beae2ab7e0b61108bd38fb5))

## [0.7.0](https://github.com/wapenshaw/herdr-customize/compare/v0.6.1...v0.7.0) (2026-10-30)


### Features

* **theme:** reset to herdr's built-in theme ([72b8565](https://github.com/wapenshaw/herdr-customize/commit/72b8565d45e48321767305f50d0be2e209ffa61e))

## [0.6.1](https://github.com/wapenshaw/herdr-customize/compare/v0.6.0...v0.6.1) (2026-10-30)


### Bug Fixes

* **bind:** take over a key held by an uninstalled plugin's action ([b285c42](https://github.com/wapenshaw/herdr-customize/commit/b285c42fcfb6a5a4b1ce37e65149287ab60b5163))

## [0.6.0](https://github.com/wapenshaw/herdr-customize/compare/v0.5.0...v0.6.0) (2026-10-30)


### Features

* **theme:** keep the palette in new terminal windows ([19b7aae](https://github.com/wapenshaw/herdr-customize/commit/19b7aae0d3ce2308f76a5f12e293600034d99415))
* **theme:** sync the theme once when the herdr server starts ([e7e4677](https://github.com/wapenshaw/herdr-customize/commit/e7e4677e05dd757b4ced4c88273bddd14ed7c266))


### Bug Fixes

* **theme:** drop the stale comment above the replaced custom block ([07df7f9](https://github.com/wapenshaw/herdr-customize/commit/07df7f99b7718b0e524cc111a3e4a83fd047a96b))

## [0.5.0](https://github.com/wapenshaw/herdr-customize/compare/v0.4.1...v0.5.0) (2026-10-26)


### Features

* **ui:** hide the themes and plugins pages until turned on in settings ([b2775eb](https://github.com/wapenshaw/herdr-customize/commit/b2775eb1bcff5e0d975b8d05cc0d15da486d159e))

## [0.4.1](https://github.com/wapenshaw/herdr-customize/compare/v0.4.0...v0.4.1) (2026-10-26)


### Bug Fixes

* **config:** lock config writes across processes ([dfc3db3](https://github.com/wapenshaw/herdr-customize/commit/dfc3db3d0b7b169ecc3c8a266331ebf3cf8052e2))
* **config:** tell new check issues from old ones by more than position ([50fc99c](https://github.com/wapenshaw/herdr-customize/commit/50fc99c69c08934bd164fcd6deb7c5c2a524ebf0))
* **fsutil:** write the daily backup atomically ([5edd614](https://github.com/wapenshaw/herdr-customize/commit/5edd614210f0ad1975d9874c74c6687523c9bb3c))
* **herdr:** give config checks and reloads a deadline ([cc35298](https://github.com/wapenshaw/herdr-customize/commit/cc3529882e6f3b199c9cfdd8ab43e34e17fdf853))
* **pluginconfig:** edit the env assignment that takes effect ([8072ba1](https://github.com/wapenshaw/herdr-customize/commit/8072ba109ba8ef58537a28068f6b6345b40036ff))
* **pluginconfig:** keep quoted toml keys whole ([e0ab2d1](https://github.com/wapenshaw/herdr-customize/commit/e0ab2d18420cedf29d76806672963b6916d39f5e))
* **pluginconfig:** read and edit files named .env ([0825f98](https://github.com/wapenshaw/herdr-customize/commit/0825f983d03a6019e2df336c0279a0930b84c3b9))
* **plugins:** keep an explicit ref's namespace in update checks ([88eda96](https://github.com/wapenshaw/herdr-customize/commit/88eda96923238d91abbe1b7537e161b50a268ef1))
* **ui:** drop a waiting pick when its setting is saved or reset ([b0ef688](https://github.com/wapenshaw/herdr-customize/commit/b0ef688f24b08ed8c37ed853984acf6262e42741))
* **ui:** save in order and finish saving before the popup closes ([29032a8](https://github.com/wapenshaw/herdr-customize/commit/29032a8d9b405dc7b48f183919b45a9792cfaf63))


### Performance

* **plugins:** share and bound update checks and report each as it lands ([c57d341](https://github.com/wapenshaw/herdr-customize/commit/c57d3416f5ed1ccfd720d0107f403ffefa94c3ee))
* **ui:** load themes when the themes page first opens ([2d4b9b0](https://github.com/wapenshaw/herdr-customize/commit/2d4b9b03acf56934ea925ffbb51e8a0d30e48a04))
* **ui:** rebuild settings and keys from the config a save wrote ([9835a72](https://github.com/wapenshaw/herdr-customize/commit/9835a728c8925e0c0f362af00a9a3bcce0a7287a))

## [0.4.0](https://github.com/wapenshaw/herdr-customize/compare/v0.3.0...v0.4.0) (2026-10-25)


### Features

* **ui:** open the popup on settings by default ([be510fb](https://github.com/wapenshaw/herdr-customize/commit/be510fb2c48a336027343d7e1f215f9e237d6574))
* **ui:** put settings first and lead the hints with tab ([cb64679](https://github.com/wapenshaw/herdr-customize/commit/cb6467937526522d74ba414f4b997649fe5632aa))

## [0.3.0](https://github.com/wapenshaw/herdr-customize/compare/v0.2.0...v0.3.0) (2026-10-25)


### Features

* **bind:** bind a key to the popup from the command line ([1028bc0](https://github.com/wapenshaw/herdr-customize/commit/1028bc02391f476a5cc388d91544281ebdbeaf91))


### Bug Fixes

* **scripts:** hash the windows download without get-filehash ([a68fa3c](https://github.com/wapenshaw/herdr-customize/commit/a68fa3cf6e139a33ebf58b0cd6c27fc677902c6e))


### Performance

* **scripts:** hide the windows download progress bar ([867ea5f](https://github.com/wapenshaw/herdr-customize/commit/867ea5f9d5e6a0ac97f006648d940d00a6e58b07))

## [0.2.0](https://github.com/wapenshaw/herdr-customize/compare/v0.1.2...v0.2.0) (2026-10-25)


### Bug Fixes

* **widgets:** keep the selected row when labels repeat ([ad2be4d](https://github.com/wapenshaw/herdr-customize/commit/ad2be4d495696efe60bc791fc3f3ba69f97f6e49))


### Performance

* **plugins:** check for updates on first view, not on open ([01c2d75](https://github.com/wapenshaw/herdr-customize/commit/01c2d75952de036cbf62ca9e76b79985c964e188))
* **settings:** reuse the check that ran during the save ([487d5aa](https://github.com/wapenshaw/herdr-customize/commit/487d5aa13eec68203ae4e3ca1a9cd4de85eea26a))
* **widgets:** rank list filters in process ([0b646a3](https://github.com/wapenshaw/herdr-customize/commit/0b646a3df87c97c686ef9f1f07a1d4e0631840aa))


### Refactoring

* drop unwraps that a pattern makes unneeded ([6406781](https://github.com/wapenshaw/herdr-customize/commit/640678108aa176102b3fba792b4c6efc6a1227ec))
* **fsutil:** format local time instead of exposing its fields ([b9bc052](https://github.com/wapenshaw/herdr-customize/commit/b9bc0525c8510644155d4379eb039cd5e382e1fc))
* **fsutil:** share the backup-then-write sequence ([3b7cc36](https://github.com/wapenshaw/herdr-customize/commit/3b7cc36c7d4608621fa135b45c57e5590af83c9c))
* **proc:** split platform PATH lookup into cfg items ([fb67a44](https://github.com/wapenshaw/herdr-customize/commit/fb67a44d9fa80337815d70d7b770f03a69f8fcbe))
* **schema:** own the installed-version lookup ([a032d99](https://github.com/wapenshaw/herdr-customize/commit/a032d997cd64f03e0613b805fa062408567e05dd))
* **schema:** write the cache atomically ([8bb283e](https://github.com/wapenshaw/herdr-customize/commit/8bb283e4caf9fd1c8e64897ba861a34b30c3c671))
* **settings:** make section headers a row variant ([788f91e](https://github.com/wapenshaw/herdr-customize/commit/788f91e5153762b9899d934ab02b0064755a1c23))
* **settings:** report saves without a relay message ([d673251](https://github.com/wapenshaw/herdr-customize/commit/d673251d2211a29c72a87b86443e452721a5e818))
* **theme:** check the palette size before reading it ([50dd0e4](https://github.com/wapenshaw/herdr-customize/commit/50dd0e49ec5db387dc51d23856e68b7a63f08482))
* **theme:** parse a palette once while resolving it ([7e78ded](https://github.com/wapenshaw/herdr-customize/commit/7e78ded700f0d7c987367c93149f11b82a4461e2))
* **theme:** share one hex color rule ([2b1c075](https://github.com/wapenshaw/herdr-customize/commit/2b1c075d9f3b72e4b50ea84f9394b158d200c383))
* **theme:** write the custom block through Store ([cef42e9](https://github.com/wapenshaw/herdr-customize/commit/cef42e9badc7445a7f250113e4e7c9d9cc3ecb79))
* **ui:** answer confirms in one place ([a73f71b](https://github.com/wapenshaw/herdr-customize/commit/a73f71b555b31a7193ca134c488d948928a84dfd))
* **ui:** borrow what the pages only read ([443ff4a](https://github.com/wapenshaw/herdr-customize/commit/443ff4a00906db1d19cb7dcaea2a9ec24add35d0))
* **ui:** carry anyhow errors through messages ([2596516](https://github.com/wapenshaw/herdr-customize/commit/259651654269e59ae62241694fff7d220c968f7b))
* **ui:** debounce picks through Ctx ([4adcb91](https://github.com/wapenshaw/herdr-customize/commit/4adcb91f6ac737565f11488c15c2f91d243c35e7))
* **ui:** keep only one cursor implementation ([6b3113a](https://github.com/wapenshaw/herdr-customize/commit/6b3113aba24791a9d8ee32e615dd39ea4bb0881d))
* **ui:** move value helpers out of the settings page ([757eba1](https://github.com/wapenshaw/herdr-customize/commit/757eba1762f6aef3cb708efcd8fe29ce554fc7c3))
* **ui:** name the filter groups and command fields as enums ([459d5ca](https://github.com/wapenshaw/herdr-customize/commit/459d5caeaaad9457b8a6c608a15edab62c5e15c4))
* **ui:** nest per-page messages ([e4538c4](https://github.com/wapenshaw/herdr-customize/commit/e4538c486b62e75174b5d16018afb1d966f4e2fe))
* **ui:** order plugin config writes with config saves ([aa95908](https://github.com/wapenshaw/herdr-customize/commit/aa95908798c1f4eb37021b51324320cf1251186a))
* **ui:** run the editor through one Ctx helper ([d506bc7](https://github.com/wapenshaw/herdr-customize/commit/d506bc768566457bc25544016d4c5f29f500024f))
* use language and library features stabilised since 1.89 ([11a4315](https://github.com/wapenshaw/herdr-customize/commit/11a43157a093e6a2b65c6daeab7ff4251ea4e60d))


### Documentation

* say what the plugin saves you from in its description ([4b41d25](https://github.com/wapenshaw/herdr-customize/commit/4b41d25c0ad357b879853f1585e8105b267b3782))

## [0.1.2](https://github.com/wapenshaw/herdr-customize/compare/v0.1.1...v0.1.2) (2026-10-25)


### Bug Fixes

* **pluginconfig:** decide a file's format before reading it ([abff8eb](https://github.com/wapenshaw/herdr-customize/commit/abff8eb3b98dee82fb234408fe7ea1b972512f3a))
* **ui:** install the terminal panic hook once ([6c8ea3d](https://github.com/wapenshaw/herdr-customize/commit/6c8ea3d0339dbb31a07e9fc73f490235514f9b2c))


### Performance

* **ui:** decode the config once per page rebuild ([8fd509e](https://github.com/wapenshaw/herdr-customize/commit/8fd509e2c08f1f7ad6c8d21d6b234fa673c6de7a))


### Refactoring

* **config:** share the keep-the-decor value replacement ([7b5d457](https://github.com/wapenshaw/herdr-customize/commit/7b5d457f83a99656a242a1e07d881dea02450ce7))
* **keys:** share the indexed suffix and clash lookup ([bba0ffb](https://github.com/wapenshaw/herdr-customize/commit/bba0ffbacee7e9ba0d080ede8d5606107f8d34d2))
* **pluginconfig:** merge entries in place and share the quote check ([16d5620](https://github.com/wapenshaw/herdr-customize/commit/16d5620502b3a0b13844cb46097f28569ac22870))
* **plugins:** share the JSON envelope and the GitHub URL ([9161497](https://github.com/wapenshaw/herdr-customize/commit/916149788756505267170a18c3212414a53a6315))
* **settings:** derive the pending path from its key ([b129d49](https://github.com/wapenshaw/herdr-customize/commit/b129d49e2fb166999a278194094492feab8ee812))
* **terminal:** replace dead-code allows with test cfgs ([f3f50d0](https://github.com/wapenshaw/herdr-customize/commit/f3f50d0b6bfb0dc9935cccb2f0c1affeb433b0ba))
* **theme:** keep token order, origin and role in one table ([c4362dd](https://github.com/wapenshaw/herdr-customize/commit/c4362ddae2d8ab2fb84991491a44b26d68ccecbd))
* **theme:** load a palette through one helper ([acbb14e](https://github.com/wapenshaw/herdr-customize/commit/acbb14ea470228e3bb62548c3587952b396339f7))
* **ui:** drop a pass-through, a dead branch and a reparse ([b4077f8](https://github.com/wapenshaw/herdr-customize/commit/b4077f8d6e3fab5aa3aadd825c628df6a705d1e4))
* **ui:** return the confirm directly from Confirm::new ([a8692d1](https://github.com/wapenshaw/herdr-customize/commit/a8692d1a6eba7a03bbb64bd40f68ad4389ab8210))
* **ui:** share the page dispatch and timer code ([c280db0](https://github.com/wapenshaw/herdr-customize/commit/c280db0ba60dfb9ac6a127d686533a5f9f73c33a))
* **ui:** take message payloads instead of cloning them ([d4a0f31](https://github.com/wapenshaw/herdr-customize/commit/d4a0f31af920687b90d3d99b4c77359c0b3d918f))
* **widgets:** share the column cut between clip and clip_str ([36c1559](https://github.com/wapenshaw/herdr-customize/commit/36c1559ed7fbb25f078cd0e34d6e4a6033a221ed))

## [0.1.1](https://github.com/wapenshaw/herdr-customize/compare/v0.1.0...v0.1.1) (2026-10-25)


### Refactoring

* **fsutil:** let localtime_r pick the time_t type ([be66fc9](https://github.com/wapenshaw/herdr-customize/commit/be66fc94a9e2c1c8c1e871d93cb2160d6679e343))

## [0.1.0] - 2026-10-25

- First release as `herdr-customize`, a standalone repository continuing
  [qintmb/herdr-theme-picker](https://github.com/qintmb/herdr-theme-picker)
  under a new plugin id. Written in Rust (ratatui, crossterm, toml_edit); the
  first Go implementation is kept on the `go` branch, and
  [docs/go-vs-rust.md](docs/go-vs-rust.md) compares them.
- Linux, macOS and Windows. Installs download a checksummed release binary
  for the platform and fall back to `cargo build` when there is none.
- One popup with four pages, opened on any page from panes and actions; fzf
  is optional and only ranks searches.
- Themes: 112 bundled palettes, live preview, your own themes from the
  clipboard or an editor, a token editor with live preview, `[theme.custom]`
  writes and terminal recoloring. Themes are grouped into in use, yours, dark
  and light with a color strip each; searching starts as you type.
- Plugins: a port of
  [speardragon/herdr-plugin-manager](https://github.com/speardragon/herdr-plugin-manager):
  update checks, single and bulk updates through Herdr's trust prompt,
  install, enable/disable, uninstall/unlink, marketplace, command logs, and
  running actions after the popup closes, in a list with a panel per plugin.
- Plugin settings: **s** on a plugin edits the TOML and `.env` files in its
  config directory, typed from their values and commented-out defaults, or
  from a `config-schema.json` the plugin ships.
- Keys: built-in bindings, custom `[[keys.command]]` entries and unbound
  plugin actions in one list, with key recording, conflict detection and
  Herdr's binding rules.
- Settings: an editor generated from Herdr's config reference for the
  installed version, plus checked whole-file editing.
- Config edits keep comments and layout (inline tables and dotted keys
  included), are refused when `herdr config check` reports a new problem,
  back up once a day, and reload Herdr.
- State lives in the `herdr-customize` plugin state directory. The client
  override variable is `HERDR_CUSTOMIZE_CLIENT_PID`.
