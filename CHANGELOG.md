# Changelog

## [1.1.2](https://github.com/brinkercode/freewheel/compare/v1.1.1...v1.1.2) (2026-11-08)


### Fixed

* --simulate no longer changes the real app's first run or What's new ([#219](https://github.com/brinkercode/freewheel/issues/219)) ([72e26b9](https://github.com/brinkercode/freewheel/commit/72e26b948a84c6211cc411efc7fc7c4183e39038))
* a backup with pages named Ärger and ärger restores, and each page key goes to its own page ([#209](https://github.com/brinkercode/freewheel/issues/209)) ([e06319d](https://github.com/brinkercode/freewheel/commit/e06319de72dc4d86a27ff73eecb7f4ce22612b29)), closes [#194](https://github.com/brinkercode/freewheel/issues/194)
* keys can be picked with the keyboard, in settings and the Simulator ([#213](https://github.com/brinkercode/freewheel/issues/213)) ([4b1b437](https://github.com/brinkercode/freewheel/commit/4b1b4376c20e8aa99a916d2f27f6a16478af1955))
* page names can't be left blank or doubled, and long names keep their buttons in reach ([#211](https://github.com/brinkercode/freewheel/issues/211)) ([426e05e](https://github.com/brinkercode/freewheel/commit/426e05eae60d96b38844785d70419a3992734e72))
* profiles for apps picked as an AppImage switch on when the app is in front ([#218](https://github.com/brinkercode/freewheel/issues/218)) ([6705cc0](https://github.com/brinkercode/freewheel/commit/6705cc04db0a01297754b35f9f143d4f8d7d54fb))
* restored Resolve program and low-battery warning apply without a restart ([#216](https://github.com/brinkercode/freewheel/issues/216)) ([ac345e5](https://github.com/brinkercode/freewheel/commit/ac345e536558f668c6ebe977af8630798e05a803)), closes [#196](https://github.com/brinkercode/freewheel/issues/196)
* shortcuts on the ß key stay on ß instead of becoming SS ([#210](https://github.com/brinkercode/freewheel/issues/210)) ([4308645](https://github.com/brinkercode/freewheel/commit/4308645e976b517075641c70cab9234be7334a83)), closes [#195](https://github.com/brinkercode/freewheel/issues/195)
* start at login works from a folder with a space in its path ([#212](https://github.com/brinkercode/freewheel/issues/212)) ([d4d73da](https://github.com/brinkercode/freewheel/commit/d4d73da8f1a4b880e94a9196115af044be9aa519))
* the Wheel tab shows the wheel button your Speed Editor has lit ([#214](https://github.com/brinkercode/freewheel/issues/214)) ([00c2321](https://github.com/brinkercode/freewheel/commit/00c2321e35fa202eef6bf1e0f90678d8afbef98d))
* updating without the AppImage says why instead of blaming the connection ([#217](https://github.com/brinkercode/freewheel/issues/217)) ([0211c61](https://github.com/brinkercode/freewheel/commit/0211c61be0149181992eb6efe9014f4f398bfcfb))

## [1.1.1](https://github.com/brinkercode/freewheel/compare/v1.1.0...v1.1.1) (2026-11-08)


### Fixed

* keys keep working when a profile has no pages ([#208](https://github.com/brinkercode/freewheel/issues/208)) ([085c741](https://github.com/brinkercode/freewheel/commit/085c741db5f44af4b0601a4f072112b5ca87f7ab)), closes [#193](https://github.com/brinkercode/freewheel/issues/193)
* release builds can only reach GitHub over HTTPS ([#192](https://github.com/brinkercode/freewheel/issues/192)) ([b859e40](https://github.com/brinkercode/freewheel/commit/b859e40d25f386b6492d7e5323edbea1b19b39b7))

## [1.1.0](https://github.com/brinkercode/freewheel/compare/v1.0.0...v1.1.0) (2026-11-08)


### New

* profiles that switch automatically for each app ([#185](https://github.com/brinkercode/freewheel/issues/185)) ([dc7acf6](https://github.com/brinkercode/freewheel/commit/dc7acf6b5e6f8734768d3cea667ea124262f513e))


### Fixed

* What's new only ever fetches notes over HTTPS ([#190](https://github.com/brinkercode/freewheel/issues/190)) ([ec6f520](https://github.com/brinkercode/freewheel/commit/ec6f52062bb509c59ad236b3cbae54fc0f871a80))
* What's new shows the release notes from GitHub ([#188](https://github.com/brinkercode/freewheel/issues/188)) ([6feac26](https://github.com/brinkercode/freewheel/commit/6feac26a8952cfb1101b4959e9532d46856ebc1f))

## [1.0.0](https://github.com/brinkercode/freewheel/compare/v0.1.0...v1.0.0) (2026-11-08)


### New

* a key can open an app ([#80](https://github.com/brinkercode/freewheel/issues/80)) ([88b87bf](https://github.com/brinkercode/freewheel/commit/88b87bff3f6a61765c9ea324e9f374f71513e935))
* a settings window to set up freewheel ([#116](https://github.com/brinkercode/freewheel/issues/116)) ([ca3baa2](https://github.com/brinkercode/freewheel/commit/ca3baa23304e7f25f9c8ec54f6754bf5caca35e0))
* a setup guide the first time freewheel runs ([#121](https://github.com/brinkercode/freewheel/issues/121)) ([89e3553](https://github.com/brinkercode/freewheel/commit/89e355323699b14493a8cc95d55c0f1e1c2c4363))
* apply the new visual design ([#128](https://github.com/brinkercode/freewheel/issues/128)) ([3d4d87c](https://github.com/brinkercode/freewheel/commit/3d4d87c015a95ea9e7ff6fab0c13a295dbfec072))
* authenticate with the Speed Editor and stay authenticated ([#62](https://github.com/brinkercode/freewheel/issues/62)) ([4126c01](https://github.com/brinkercode/freewheel/commit/4126c0170bf1c17a2510dd4aab433f91bea89e3f))
* back up and restore your keys ([#118](https://github.com/brinkercode/freewheel/issues/118)) ([3a21c9f](https://github.com/brinkercode/freewheel/commit/3a21c9fdb13431cdaa786b71d2854dc0004cb499))
* **backup:** a short backup file you can edit by hand, with a documented format ([#154](https://github.com/brinkercode/freewheel/issues/154)) ([04fb2b3](https://github.com/brinkercode/freewheel/commit/04fb2b3358a055d763fc5e5b6980a9222205f296))
* battery level in the tray, and a low-battery notification ([#101](https://github.com/brinkercode/freewheel/issues/101)) ([8f7f917](https://github.com/brinkercode/freewheel/commit/8f7f917f8a1bab0d25833c3739e7b5076d2dd2e3))
* built-in display brightness on macOS ([#100](https://github.com/brinkercode/freewheel/issues/100)) ([1c6015e](https://github.com/brinkercode/freewheel/commit/1c6015e2bf0466ed71cfb4b88d0b262cfe5509ca))
* connect to the Speed Editor over USB and Bluetooth ([#71](https://github.com/brinkercode/freewheel/issues/71)) ([f824e09](https://github.com/brinkercode/freewheel/commit/f824e0960884dbc6bb3d2bf34db1a414297db056))
* **device:** a Device test view, and wheel modes and sleep/wake work on a real Speed Editor ([#142](https://github.com/brinkercode/freewheel/issues/142)) ([1551d1d](https://github.com/brinkercode/freewheel/commit/1551d1dde613b4c1d1f43bda3acbc83d745f9468))
* **device:** the Speed Editor's key and wheel lights show what is on ([#141](https://github.com/brinkercode/freewheel/issues/141)) ([09fd39e](https://github.com/brinkercode/freewheel/commit/09fd39ee3b21dd5c5351138f245cacb8b9f8819c))
* download the Windows exe or the macOS .dmg directly, no zips ([#173](https://github.com/brinkercode/freewheel/issues/173)) ([58b42fa](https://github.com/brinkercode/freewheel/commit/58b42fa59a04cf2d379a9f7556c1697caf7d2f84))
* drive the Speed Editor's lights and wheel mode ([#70](https://github.com/brinkercode/freewheel/issues/70)) ([e2451f9](https://github.com/brinkercode/freewheel/commit/e2451f9bd02b1d559e0717aa4bb598e745606629))
* editors for opening apps, pages, wheel modes and overrides ([#127](https://github.com/brinkercode/freewheel/issues/127)) ([85c39b6](https://github.com/brinkercode/freewheel/commit/85c39b64c907ca60ad86fc446f1d16c2839a7a33))
* every button and switch in the settings window now does something ([#138](https://github.com/brinkercode/freewheel/issues/138)) ([bb72b16](https://github.com/brinkercode/freewheel/commit/bb72b1623c9f19b30a3242001e0041a3ff8c11e3))
* freewheel can start at login ([#99](https://github.com/brinkercode/freewheel/issues/99)) ([57d0c82](https://github.com/brinkercode/freewheel/commit/57d0c82b452396398f164f19f049debc7cabdf63))
* freewheel can update itself ([#75](https://github.com/brinkercode/freewheel/issues/75)) ([5a1100d](https://github.com/brinkercode/freewheel/commit/5a1100dba5a29df8b397ea4bfd83d048beb0c99d))
* freewheel steps aside while DaVinci Resolve is open ([#97](https://github.com/brinkercode/freewheel/issues/97)) ([bcedce6](https://github.com/brinkercode/freewheel/commit/bcedce69d0c53c147658d4768dcff1e2ae362bf0))
* held keys are released when freewheel is switched off or the Speed Editor disconnects ([#83](https://github.com/brinkercode/freewheel/issues/83)) ([30ead39](https://github.com/brinkercode/freewheel/commit/30ead397d2d53faf6589b7303fa97ff569171277))
* hold or latch a modifier key on its own (Shift, Ctrl/Cmd, Alt) ([#129](https://github.com/brinkercode/freewheel/issues/129)) ([2792e0a](https://github.com/brinkercode/freewheel/commit/2792e0a4498ac91ee01ec70bbc86aa415f18d947))
* keys can press a shortcut, hold a key, or turn a key on and off ([#74](https://github.com/brinkercode/freewheel/issues/74)) ([03c4664](https://github.com/brinkercode/freewheel/commit/03c4664b7ea22eb920c523a7e7dc16dbee785376))
* Keys tab with a drawing of the Speed Editor, a list of keys and the key panel ([#123](https://github.com/brinkercode/freewheel/issues/123)) ([f5b8dc0](https://github.com/brinkercode/freewheel/commit/f5b8dc08491a85b9e162357527ee5099c12da6ad))
* **linux:** an AppImage that updates itself in place, and a lighter idle tray ([#172](https://github.com/brinkercode/freewheel/issues/172)) ([6a817b1](https://github.com/brinkercode/freewheel/commit/6a817b136fe84ec5b6f0203c1d3a59616d9eb02d))
* **linux:** freewheel runs on Linux: the Speed Editor, keystrokes, apps, volume, brightness and the tray ([#150](https://github.com/brinkercode/freewheel/issues/150)) ([ffbeb3f](https://github.com/brinkercode/freewheel/commit/ffbeb3fe85651aea37a49e65ff00b946f1bce54e))
* log files for troubleshooting ([#67](https://github.com/brinkercode/freewheel/issues/67)) ([4ceb306](https://github.com/brinkercode/freewheel/commit/4ceb30624ccb94fe4f4a2dd7a459c98fcdfbf312))
* macOS permissions in the settings window ([#117](https://github.com/brinkercode/freewheel/issues/117)) ([ce53e82](https://github.com/brinkercode/freewheel/commit/ce53e828f4e7387bb75887ff1eadd9fb65c25047))
* monitor brightness on macOS ([#93](https://github.com/brinkercode/freewheel/issues/93)) ([119d518](https://github.com/brinkercode/freewheel/commit/119d518038e160fde024edf6bfbc3f03ffc84378))
* on macOS, app volume follows the app in front ([#92](https://github.com/brinkercode/freewheel/issues/92)) ([1fd8da9](https://github.com/brinkercode/freewheel/commit/1fd8da91bb56c7ec19f7430384a29f6cfd02b16d))
* on macOS, freewheel tells you when it needs a permission ([#102](https://github.com/brinkercode/freewheel/issues/102)) ([24b5ba4](https://github.com/brinkercode/freewheel/commit/24b5ba4b4eb760d1b1df6e8ff51bcb0a5bd24610))
* pages of keys, with automatic return ([#79](https://github.com/brinkercode/freewheel/issues/79)) ([1ac0b85](https://github.com/brinkercode/freewheel/commit/1ac0b85887660ef85113ee6bf7a9324d7e5fdac7))
* read the Speed Editor's keys and wheel ([#56](https://github.com/brinkercode/freewheel/issues/56)) ([0982b73](https://github.com/brinkercode/freewheel/commit/0982b73dff76bd7794a3938f04766ed8df2e1416))
* reconnect automatically, with an On/Off switch and the connection status ([#72](https://github.com/brinkercode/freewheel/issues/72)) ([1c87e0d](https://github.com/brinkercode/freewheel/commit/1c87e0d4047ef5d2e6a9e819a1ae298831a0a0d3))
* screen brightness on Windows, for monitors and laptop screens ([#88](https://github.com/brinkercode/freewheel/issues/88)) ([6ffec87](https://github.com/brinkercode/freewheel/commit/6ffec872c60e1a190d166d2c02f8989d22121ad7))
* send keystrokes to the app in front ([#73](https://github.com/brinkercode/freewheel/issues/73)) ([46083df](https://github.com/brinkercode/freewheel/commit/46083dfaa4c76495d023ed63958098c4fd45c832))
* settings for opening apps, pages, wheel modes and the wheel's volume and brightness ([#122](https://github.com/brinkercode/freewheel/issues/122)) ([7545419](https://github.com/brinkercode/freewheel/commit/7545419cb070619c822e3ef740617ab7570e9a33))
* settings window and simulator redesign, plus stepping aside for Resolve, detailed logs, log export and volume/brightness wheel modes ([#130](https://github.com/brinkercode/freewheel/issues/130)) ([60bebec](https://github.com/brinkercode/freewheel/commit/60bebec4ab32d5b39874b1ef0e096b600fc45cac))
* **simulate:** the Simulator does actions for real by default, and the log says what happened ([#155](https://github.com/brinkercode/freewheel/issues/155)) ([cd04a6c](https://github.com/brinkercode/freewheel/commit/cd04a6c081c9375ea08a0d4143919c6e0d4a9b96))
* simulator mode: try freewheel without a Speed Editor ([#98](https://github.com/brinkercode/freewheel/issues/98)) ([670a311](https://github.com/brinkercode/freewheel/commit/670a3117426331f1af5fd5d192f947d54e61915a))
* system volume from the wheel on macOS ([#89](https://github.com/brinkercode/freewheel/issues/89)) ([9c417dc](https://github.com/brinkercode/freewheel/commit/9c417dcf22fbde88693a7aeeb4d785a3f9ff6d72))
* system volume from the wheel on Windows ([#86](https://github.com/brinkercode/freewheel/issues/86)) ([258d4a0](https://github.com/brinkercode/freewheel/commit/258d4a00069fb19cad18fe75640a955f25f997d2))
* the General tab in the settings window ([#119](https://github.com/brinkercode/freewheel/issues/119)) ([28b0911](https://github.com/brinkercode/freewheel/commit/28b09117566c9412e03075abaaa682b46a22e0cf))
* the key lights show what is on ([#84](https://github.com/brinkercode/freewheel/issues/84)) ([fc5f231](https://github.com/brinkercode/freewheel/commit/fc5f231e5f92b4897b5466bfc68c3c46a1ddccc8))
* the settings window shows the live status ([#109](https://github.com/brinkercode/freewheel/issues/109)) ([3161cb7](https://github.com/brinkercode/freewheel/commit/3161cb76e063dd10878068c1989579950a9eab23))
* the simulator can try DaVinci Resolve, permissions and updates ([#104](https://github.com/brinkercode/freewheel/issues/104)) ([d5727ba](https://github.com/brinkercode/freewheel/commit/d5727baee1c8d25c08e961f33477a3c9b32c7334))
* the volume of the app in front from the wheel on Windows ([#85](https://github.com/brinkercode/freewheel/issues/85)) ([4fa9155](https://github.com/brinkercode/freewheel/commit/4fa91554f583449a1520bdad6b8414831b473bb5))
* the volume of the app in front on macOS ([#94](https://github.com/brinkercode/freewheel/issues/94)) ([faac1cc](https://github.com/brinkercode/freewheel/commit/faac1ccc5ef5c64d110aedc2d9954d632dfb5b2e))
* the wheel can control volume or brightness ([#81](https://github.com/brinkercode/freewheel/issues/81)) ([1b0288b](https://github.com/brinkercode/freewheel/commit/1b0288b9a23c3a8a6ec5417488fcb2b0313d1a31))
* tray icon with On/Off and Quit; only one freewheel runs at a time ([#96](https://github.com/brinkercode/freewheel/issues/96)) ([cecc972](https://github.com/brinkercode/freewheel/commit/cecc972437878bd36fa9759ab077e0780115f5f1))
* **tray:** a full tray menu with updates, real app icons in the app picker, and system notifications ([#143](https://github.com/brinkercode/freewheel/issues/143)) ([99d0871](https://github.com/brinkercode/freewheel/commit/99d0871521dd62fd1c37b62f7829ac40f074968b))
* **updates:** Check for updates works and shows each step, also in the Simulator ([#137](https://github.com/brinkercode/freewheel/issues/137)) ([d6b215f](https://github.com/brinkercode/freewheel/commit/d6b215fa68dd40c2634d11d265b86286f243da8f))
* **updates:** no installer: freewheel is one portable app that updates itself ([#145](https://github.com/brinkercode/freewheel/issues/145)) ([bfd7c8c](https://github.com/brinkercode/freewheel/commit/bfd7c8ce624618c4a77899ce327135b501b70d1a))
* **updates:** release notes grouped as New, Fixed and Faster, and plainer in What's new ([#180](https://github.com/brinkercode/freewheel/issues/180)) ([78c9371](https://github.com/brinkercode/freewheel/commit/78c9371fc828b189b760697363ba12871a34fabf))
* **updates:** signed automatic updates, and What's new after an update ([#131](https://github.com/brinkercode/freewheel/issues/131)) ([7f0c094](https://github.com/brinkercode/freewheel/commit/7f0c0945845a2911265cef24bf0db0b9cc1cd470))
* **updates:** What's new shows clean, formatted release notes with working links ([#153](https://github.com/brinkercode/freewheel/issues/153)) ([c83c3ae](https://github.com/brinkercode/freewheel/commit/c83c3aecc5ae9a68780301a7f7be2e60b63051fe))
* versioned settings with defaults ([#65](https://github.com/brinkercode/freewheel/issues/65)) ([6513edd](https://github.com/brinkercode/freewheel/commit/6513edd8506aa2ae9b8caf00e20b19669bac0673))
* wheel modes: jog, scroll and shuttle ([#78](https://github.com/brinkercode/freewheel/issues/78)) ([1a0e584](https://github.com/brinkercode/freewheel/commit/1a0e58433b4929d24eca4033daa04c7a0ead3f71))
* **window:** a seamless title bar, and Open System Settings now opens System Settings ([#136](https://github.com/brinkercode/freewheel/issues/136)) ([322e38c](https://github.com/brinkercode/freewheel/commit/322e38cd00b9abe026063af2adbc4d4371f7b649))


### Fixed

* **backup:** a key that opens a different app on each OS keeps every app through Restore and Back up ([#174](https://github.com/brinkercode/freewheel/issues/174)) ([e23eb2f](https://github.com/brinkercode/freewheel/commit/e23eb2fcebe551adaa58cbff276f56be026f51ef))
* **device:** a real Speed Editor now connects ([#140](https://github.com/brinkercode/freewheel/issues/140)) ([7163da0](https://github.com/brinkercode/freewheel/commit/7163da01a280e9ee9dfb9d95975dd4fdab025fd8))
* freewheel runs on Windows with nothing else installed ([#108](https://github.com/brinkercode/freewheel/issues/108)) ([ec9ec35](https://github.com/brinkercode/freewheel/commit/ec9ec35f4b11acfe454a668ab6d45473603fc4c4))
* **simulate:** --simulate opens its own window, not an older running freewheel ([#134](https://github.com/brinkercode/freewheel/issues/134)) ([508fb04](https://github.com/brinkercode/freewheel/commit/508fb04bf2613d5530532742ba74187507ac597e))
* **simulate:** a key pressed twice quickly in the Simulator counts as two presses ([#187](https://github.com/brinkercode/freewheel/issues/187)) ([dd3c79a](https://github.com/brinkercode/freewheel/commit/dd3c79a9c2160f57967fbe09b27b0c669a7fd125))
* **simulate:** the app no longer freezes after Get started in the Simulator ([#135](https://github.com/brinkercode/freewheel/issues/135)) ([66a076c](https://github.com/brinkercode/freewheel/commit/66a076c95cdcae8a76fe7e9534659e65462ffcba))
* **simulate:** the Simulator's tray menu no longer jumps back to an older state ([#183](https://github.com/brinkercode/freewheel/issues/183)) ([f68e06e](https://github.com/brinkercode/freewheel/commit/f68e06ed63782b9b07add4a11e65cd2fbd87b872))
* the header status fits at the default window size, and a reopened settings window follows the tray ([#152](https://github.com/brinkercode/freewheel/issues/152)) ([58bafa3](https://github.com/brinkercode/freewheel/commit/58bafa335972c044c5495ba8ce7df013c878cbba))
* the header status never overlaps the tabs, the tray and the window share one On/Off, and the tray icons are pink ([#146](https://github.com/brinkercode/freewheel/issues/146)) ([c4db832](https://github.com/brinkercode/freewheel/commit/c4db8329da9de66577641d937d3b28d8ff9b8d2d))
* **updates:** update notes never show HTML comment markers ([#181](https://github.com/brinkercode/freewheel/issues/181)) ([354367b](https://github.com/brinkercode/freewheel/commit/354367b946b294ef2306c9b0e86aeb13924a2599))
* **window:** the minimise, maximise and close buttons sit in line with the tabs ([#139](https://github.com/brinkercode/freewheel/issues/139)) ([db02cac](https://github.com/brinkercode/freewheel/commit/db02cace3fa118ba1387f9df97cca324f7cf8def))


### Faster

* freewheel's memory, CPU and disk use on Windows are measured and kept small ([#144](https://github.com/brinkercode/freewheel/issues/144)) ([8a72c5d](https://github.com/brinkercode/freewheel/commit/8a72c5d9fad2431cc3512889c6947c900b3f88b5))


### Miscellaneous Chores

* the first release is 1.0.0 ([#186](https://github.com/brinkercode/freewheel/issues/186)) ([69b4b69](https://github.com/brinkercode/freewheel/commit/69b4b6915464ff58d8dbd4534055f5822563bdfb))
