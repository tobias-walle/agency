## [1.7.1](https://github.com/tobias-walle/agency/compare/v1.7.0...v1.7.1) (2025-12-19)


### Bug Fixes

* display stored base_branch in tasks command instead of current HEAD ([b949d83](https://github.com/tobias-walle/agency/commit/b949d83d264b284eab8b0a742a2c5895c6591c2c))
* resolve clippy pedantic warnings ([0bc2acd](https://github.com/tobias-walle/agency/commit/0bc2acdb6e28f33093a039d94597005df92340c0))

# [1.7.0](https://github.com/tobias-walle/agency/compare/v1.6.1...v1.7.0) (2025-12-19)


### Features

* add `agency exec <task> -- <cmd>` ([0737ffe](https://github.com/tobias-walle/agency/commit/0737ffee6c58fadae82950503b4c1139c193b460))

## [1.6.1](https://github.com/tobias-walle/agency/compare/v1.6.0...v1.6.1) (2025-12-16)


### Bug Fixes

* resolve to git repo root instead of cwd in worktrees ([4dfbc18](https://github.com/tobias-walle/agency/commit/4dfbc18e5264b40319e9b39adaf12d3cc6070233))

# [1.6.0](https://github.com/tobias-walle/agency/compare/v1.5.0...v1.6.0) (2025-11-24)


### Features

* start agent directly on "new" with --draft and don't open the editor ([a2f43a6](https://github.com/tobias-walle/agency/commit/a2f43a6431be109d382f2ecae22172a5f814cf1b))
* TUI Start and New+Start run sessions without attach ([316fcd3](https://github.com/tobias-walle/agency/commit/316fcd34450af54ff0e3de8446ca17eb8c8b762a))

# [1.5.0](https://github.com/tobias-walle/agency/compare/v1.4.0...v1.5.0) (2025-11-16)


### Features

* add --follow option to attach that follows the tasks in the TUI ([825d5ed](https://github.com/tobias-walle/agency/commit/825d5ed447eab6d195ea09fabad03b6e7ad11612))
* **tui:** improve ux for task creation and add description box ([d5c34a7](https://github.com/tobias-walle/agency/commit/d5c34a7823f999296aea68a61bebded4888ec858))

# [1.4.0](https://github.com/tobias-walle/agency/compare/v1.3.0...v1.4.0) (2025-11-15)


### Bug Fixes

* make new prompt clearer ([12664f1](https://github.com/tobias-walle/agency/commit/12664f195ba3df110610dc2756b8806999da4df0))


### Features

* run agents via shell send-keys to improve ctrl-c/z behavior ([1409e6c](https://github.com/tobias-walle/agency/commit/1409e6c5f5474c30de65a045ef00b7ade0ed9e9d))

# [1.3.0](https://github.com/tobias-walle/agency/compare/v1.2.0...v1.3.0) (2025-11-15)


### Bug Fixes

* autostart tmux server ([5949f00](https://github.com/tobias-walle/agency/commit/5949f00c113a2aefae3a87c43dc129f81e907494))
* fix tests ([aab2a32](https://github.com/tobias-walle/agency/commit/aab2a32d9fd61f1466c1204328967ec9ad962cad))


### Features

* add description and no-attach to new/start to enable non-interactive creation and optional detach ([839e445](https://github.com/tobias-walle/agency/commit/839e445595afe83d49dd84a0e1934c014f2befd5))
* autostart daemon and restart on version mismatch ([4c0d535](https://github.com/tobias-walle/agency/commit/4c0d5353a45bb567767913c7980d7e1b34f3c28f))
* config-based editor, tasks cmd, config cmd, better git errors, setup docs ([ed9b303](https://github.com/tobias-walle/agency/commit/ed9b303ad5e9b22fc0c8aa61611186a6fab1f371))
* show changes in overview ([c96d532](https://github.com/tobias-walle/agency/commit/c96d532087341505e6d182a535ab9c11e3f17a4b))

# [1.2.0](https://github.com/tobias-walle/agency/compare/v1.1.0...v1.2.0) (2025-11-14)


### Bug Fixes

* fix warnings ([5e8ce0b](https://github.com/tobias-walle/agency/commit/5e8ce0b5ebe3067b967fbbe80df66d6f8ef682a3))
* improve tmux defaults ([57e1f19](https://github.com/tobias-walle/agency/commit/57e1f192ebb4036412f54dbf5ffde9216381fad0))
* small tui ux improvements ([909f07c](https://github.com/tobias-walle/agency/commit/909f07cd4817b64fbcd54acadc0c12406c21efd9))


### Features

* fail merge when task has no changes ([290bc68](https://github.com/tobias-walle/agency/commit/290bc68913e585bab6fea7734689fd5de4192df2))
* migrate from PTY to tmux sessions to avoid edge cases ([77655dc](https://github.com/tobias-walle/agency/commit/77655dc498073bc2ad26fff4fb107b4b01466e9e))
* show tmux status bar and real detach binding; close on exit ([f4aaa86](https://github.com/tobias-walle/agency/commit/f4aaa86322c94baf156199bfd4ed65e10a7f4899))

# [1.1.0](https://github.com/tobias-walle/agency/compare/v1.0.1...v1.1.0) (2025-11-12)


### Bug Fixes

* resolve small styling issues in setup prompts ([b424eae](https://github.com/tobias-walle/agency/commit/b424eaef26acaac55d355e6037c2d2178ebb8170))


### Features

* add shell to setup wizard ([035dc63](https://github.com/tobias-walle/agency/commit/035dc63aebef5b8c887140ee9bda26c3dbbc5f02))

## [1.0.1](https://github.com/tobias-walle/agency/compare/v1.0.0...v1.0.1) (2025-11-12)


### Bug Fixes

* optimise column widths ([08e8173](https://github.com/tobias-walle/agency/commit/08e8173336165c9fe1d627d339c8022909e73b81))

# 1.0.0 (2025-11-12)


### Bug Fixes

* allow merge with unstaged changes ([b8036a3](https://github.com/tobias-walle/agency/commit/b8036a315c531b8aed60dbcece620d6c019a2bcd))
* allow stop at handshake and add ack with clearer logging to fix stop errors ([e3a4a6c](https://github.com/tobias-walle/agency/commit/e3a4a6ce84ec5e8e5bb65132813e891519320ea0))
* always attach to prevent missing cursor issues ([e86b456](https://github.com/tobias-walle/agency/commit/e86b456ee7551176baf3afb26ab6e4b830523bdd))
* avoid flickering in TUI then opening interactive program ([64a3dcc](https://github.com/tobias-walle/agency/commit/64a3dcc66b1b2610b3cf1e9f3f4521ed16bffa48))
* centralize notification handling and fix bug that tui wasn't updated after merge ([62abe57](https://github.com/tobias-walle/agency/commit/62abe577155d4b9283b851fe7ca465b554e1e462))
* **cli,core,tests:** drop title usage, align help, and make base branch default follow current HEAD ([067294e](https://github.com/tobias-walle/agency/commit/067294ebfaecfc3f1d2c14d1b097d3b8203e362e))
* **cli:** show actionable error when daemon is unreachable ([fc16fc5](https://github.com/tobias-walle/agency/commit/fc16fc53d4da2aa4b697fc92f2b826dcab41d419))
* **core/daemon:** avoid unwraps in RPC serialization ([d7422fe](https://github.com/tobias-walle/agency/commit/d7422fe404734af8259147fb25a8993b3bff9abd))
* don't copy folders if not included ([9f71d49](https://github.com/tobias-walle/agency/commit/9f71d496e5da7d879e13bb47bf814537be5854b6))
* enforce slug starting letter and address clippy warnings ([342492e](https://github.com/tobias-walle/agency/commit/342492e91014291c2206d2b2665a643a3442afe7))
* fix agency ps table formatting ([755d38d](https://github.com/tobias-walle/agency/commit/755d38db579e11c085518a150742b64cb67246fa))
* fix control chars issues (especially with TUIs) after attach (PLN-8) ([6d6980d](https://github.com/tobias-walle/agency/commit/6d6980d4133e64c4783ace5bec469f6ad6fa7d54))
* fix control code rendering ([fdff722](https://github.com/tobias-walle/agency/commit/fdff722045b69c37ba79665d8fe618582c8ce9ec))
* fix cursor request handling with codex ([349ef52](https://github.com/tobias-walle/agency/commit/349ef52e07520402a5e0d2aae48f3522b2e20707))
* fix idle detection and add logs ([cf3dcd6](https://github.com/tobias-walle/agency/commit/cf3dcd6bdabbf82d20c8d8c21e354246012377fb))
* fix just test command ([d25b498](https://github.com/tobias-walle/agency/commit/d25b498a3f9677c2277ef18b1806563df09805cc))
* fix scroll issue in pty ([8ce81d4](https://github.com/tobias-walle/agency/commit/8ce81d4d4a9967e4a19d108d7721ccaefb0468e4))
* fix tests ([507f214](https://github.com/tobias-walle/agency/commit/507f21420dc0a0cacf9810731edbb81750e5746d))
* fix warnings ([5ae7f8e](https://github.com/tobias-walle/agency/commit/5ae7f8eaafd2c1dae022e2d5acc0e0e56fddc7e0))
* **gc:** avoid deleting agency/* branches when a worktree exists and prune worktrees first ([ddddb74](https://github.com/tobias-walle/agency/commit/ddddb74f67c4d8e530509128e9eef030879c528e))
* improve error handling if daemon is stopped ([002febd](https://github.com/tobias-walle/agency/commit/002febd605258fc13979fe0a1bbd48bbfef7ce70))
* leave alternate screen during interactive to restore scrollback ([3af07da](https://github.com/tobias-walle/agency/commit/3af07dae1dabd7b0a38fcc9882675198502694da))
* make ctrl-q detach work with extended keyboard via termwiz ([10700b6](https://github.com/tobias-walle/agency/commit/10700b6400c5b99035d55748359a222addbc3798))
* make enter reliably restart session and re-arm after each exit ([1396c31](https://github.com/tobias-walle/agency/commit/1396c31b7b9f5da1134913f12782049cbd598a3e))
* reattach to session on same task ([aca69a2](https://github.com/tobias-walle/agency/commit/aca69a25272cef96d817e99d44c5130e590507b5))
* remove newline from frontmatter ([64f2102](https://github.com/tobias-walle/agency/commit/64f2102745cf88f04a2005f5608fa53d06dbe559))
* remove redudant tests ([459b8eb](https://github.com/tobias-walle/agency/commit/459b8eb4edce5140ca929417eb777d324934c85a))
* resolve .agency root to main repo when run from worktrees ([29b51ac](https://github.com/tobias-walle/agency/commit/29b51ac2a4b1e23a51b40fe4ba3c124176a23800))
* resolve clippy pedantic warnings and improve task markdown utils ([eb0a416](https://github.com/tobias-walle/agency/commit/eb0a416eb2e055a74e5e610d037368e1a17cc296))
* resolve pedantic clippy lints to make just fix green ([65018c5](https://github.com/tobias-walle/agency/commit/65018c58cda2ffaa035fc7fe110133f50e73b969))
* restore terminal state to avoid leftover extended keyboard modes ([4c03a85](https://github.com/tobias-walle/agency/commit/4c03a85b9d491dd6c33afa7704bed2fc37ce84ab))
* revert workaround ([5f5189d](https://github.com/tobias-walle/agency/commit/5f5189ddc98c6ac6b8818c45171c9e7492b3ad51))
* track session status and broadcast after restart to update TUI ([d31e132](https://github.com/tobias-walle/agency/commit/d31e132cba004216482daf1bcc50b9218e00b239))
* update fs after merge ([2df0da5](https://github.com/tobias-walle/agency/commit/2df0da5ef6492c318bd636bbc34d0ebb97e4155f))


### Features

* add .gitignore entries on `init` ([d5207b8](https://github.com/tobias-walle/agency/commit/d5207b8db6653d79c7095217a7fe52c1d8139128))
* add `agency shell` command ([2d6e9bd](https://github.com/tobias-walle/agency/commit/2d6e9bdbae22d5912e6ba15336579fd69209e2cd))
* add agency merge & agency open ([cfbe255](https://github.com/tobias-walle/agency/commit/cfbe255db7ebccf2de81c37fcee1b105843abb35))
* add agents to config ([727c662](https://github.com/tobias-walle/agency/commit/727c6624cbf59a72484f11256560a7c56f0236ff))
* add bootstrap cmd to prepare worktrees and drop attach prepare-only ([1e849fc](https://github.com/tobias-walle/agency/commit/1e849fc073787edfd75bb3a4c6b3753c97d2af0f))
* add complete command and persistent Completed status override ([ac590e6](https://github.com/tobias-walle/agency/commit/ac590e6047fa859df67158f96a46e2af35e796a0))
* add config loading ([1bb4bc1](https://github.com/tobias-walle/agency/commit/1bb4bc10f9681362ced455dc15154538d2479aaf))
* add event-driven tui with subscribe and input overlay ([9466e1b](https://github.com/tobias-walle/agency/commit/9466e1b39f8bb7ed9bfa62a9be3367b02341d548))
* add idle detection ([72e94f7](https://github.com/tobias-walle/agency/commit/72e94f7f7188db3e9667b8b3dea084cb574100ab))
* add missing commands to cli ([539618c](https://github.com/tobias-walle/agency/commit/539618c9586e0987be8fbe1805d8b46d7edc6cc7))
* add multi session pty management ([437d133](https://github.com/tobias-walle/agency/commit/437d133b3ce6b4c8361b05dde6a15af22b58f6ce))
* add opencode and autoattach on task start ([e6ab824](https://github.com/tobias-walle/agency/commit/e6ab8245583509d307ebb0aed5705e82b2388ad3))
* add option to select agent in TUI ([d145e02](https://github.com/tobias-walle/agency/commit/d145e026051cbb860feaf977427edb648a66f540))
* add override config for socket path ([3065a09](https://github.com/tobias-walle/agency/commit/3065a09ee2855058221115b5920d3c46489a7499))
* add RUST_BEST_PRACTICES and fix tests and lints ([7e7b6c7](https://github.com/tobias-walle/agency/commit/7e7b6c78fa5eb2b629324281f855053a29e5e1a9))
* add setup cmd support ([9dc5d7b](https://github.com/tobias-walle/agency/commit/9dc5d7b416e6b7916f93dccbab5359e4ff7f7881))
* add STOPPED state ([ecd98ff](https://github.com/tobias-walle/agency/commit/ecd98ff7374f22c4e9fb2d871cbcdd4dca83dafc))
* add TUI (phase 1) ([6514fd4](https://github.com/tobias-walle/agency/commit/6514fd4f61c09f4187e820a33248db3b63ee47b5))
* add worktrees, branches and new CLI commands per PLN-3 with centralized task resolution and tests ([d3c0e87](https://github.com/tobias-walle/agency/commit/d3c0e871748c70ab6018fe851f59f72a0901d2fd))
* **cli,core,docs:** add init scaffolding and git helpers to prepare setup ([445fff3](https://github.com/tobias-walle/agency/commit/445fff34cac8e68a2f490ace4e5bf6a6d8128df5))
* **cli,core,docs:** finalize phase 10 with PTY resize, logs and docs ([7297598](https://github.com/tobias-walle/agency/commit/72975981cc9c3ca1d92d06b9ac354ff3fcb386a1))
* **cli,core:** add daemon start/stop/run and shutdown rpc to improve ux ([02fe78b](https://github.com/tobias-walle/agency/commit/02fe78b7ab3b6db235bb5a0ee498542366e1b890))
* **cli,tests:** autostart daemon add restart and start tasks by default ([83d1a06](https://github.com/tobias-walle/agency/commit/83d1a0603a2e352a376d39453b445d398d0a1504))
* **cli:** add reset footer on detach to restore terminal state and cursor color ([d5aed57](https://github.com/tobias-walle/agency/commit/d5aed57fa82bae5ede586a6f681165ad2f92f109))
* **cli:** add uds json-rpc client to query daemon status for basic ux ([0146411](https://github.com/tobias-walle/agency/commit/0146411df097a83674989019f3a762c9c598aa9f))
* **cli:** modularize stdin handling and file logging for reliable attach ([534415d](https://github.com/tobias-walle/agency/commit/534415d62cacb07295e7490da4e52d9b87778f71))
* copy ignored files & some folders on worktree creation ([ee13053](https://github.com/tobias-walle/agency/commit/ee1305384bb901f5eacf56ac4eb1e6cc95281ea5))
* copy pty-demo and add attach/daemon to enable PTY sessions ([ab5c2e6](https://github.com/tobias-walle/agency/commit/ab5c2e666a69f3e38fd52e33bee38d33c1bec6ca))
* **core,apps:** add structured JSON logging and mark phase 5 done ([e4b038e](https://github.com/tobias-walle/agency/commit/e4b038eeb815cdde1b643debe5aef3d0aa1d9738))
* **core,daemon,cli,docs:** implement real git worktrees and add CLI helpers to enable isolated workspaces ([69af741](https://github.com/tobias-walle/agency/commit/69af74173db72e41db937bec1aaa86953feeaa27))
* **core/config,fs:** add Config load/merge, socket path resolution, and .orchestra layout helpers; mark plan phase 4 done ([4509a74](https://github.com/tobias-walle/agency/commit/4509a743bf6d7d529680122947b80d52c47b72f5))
* **core/daemon:** add minimal JSON-RPC over UDS with daemon.status and e2e test ([ee6f832](https://github.com/tobias-walle/agency/commit/ee6f832267600528803d273f1940e27f6ef0ad37))
* **core/daemon:** switch to jsonrpsee over UDS to improve correctness ([8359f4d](https://github.com/tobias-walle/agency/commit/8359f4d9794b4fa1fc72563959cea77b9a29c1a4))
* **core/domain:** add Task/Status YAML front matter, filename parsing, and transition guards with tests ([fe059e5](https://github.com/tobias-walle/agency/commit/fe059e5c1d2c2ec720cadb6cce53c7e8ff45a670))
* **core:** add agent runner to resolve configured actions ([8e97206](https://github.com/tobias-walle/agency/commit/8e972066fa6239e8e7039873ba030b41dbe649aa))
* **core:** add task lifecycle RPCs to enable draft-to-running flow ([931278c](https://github.com/tobias-walle/agency/commit/931278ce74db378199c46c70ca18f40730af5d16))
* **core:** resume running tasks on daemon boot using resume root ([c34a8ec](https://github.com/tobias-walle/agency/commit/c34a8ec347aa0762c9221650fea3cbf371650be9))
* create setup, defaults and init command ([85c809f](https://github.com/tobias-walle/agency/commit/85c809fea4476963737755440aae1443a83426f6))
* **daemon:** mark running sessions stopped during resume ([9322423](https://github.com/tobias-walle/agency/commit/93224239828f1d433100ea1a3c305f6a045f9fc6))
* default to attaching new tasks with --no-attach opt-out ([af971d3](https://github.com/tobias-walle/agency/commit/af971d3e92e8a06b71c0f2b24ab8229d371aee8e))
* enable task creation via new command with tty-aware output ([4ea024b](https://github.com/tobias-walle/agency/commit/4ea024b343f0ce313d75a1e368150b8922caec07))
* expose AGENCY_ROOT env to agents and add to codex --add-dir ([46ee3b7](https://github.com/tobias-walle/agency/commit/46ee3b73734516edb6a3537443e87bf111ae076e))
* extend agency default toml ([8df4e9d](https://github.com/tobias-walle/agency/commit/8df4e9df304ed35fff7ab90c9c1a56205a158b99))
* fix pty handling ([2223872](https://github.com/tobias-walle/agency/commit/2223872f53726c2e3f06999d6c847e8b3cf469db))
* improve git merge logs ([6a7e164](https://github.com/tobias-walle/agency/commit/6a7e164cdc5821b77f56b227d29b5de920611507))
* improve logging ([781b0be](https://github.com/tobias-walle/agency/commit/781b0bebc3b6e6280a1aca86676df6b4be9377fa))
* improve tui UX and add Stopped state ([8373f0b](https://github.com/tobias-walle/agency/commit/8373f0bda538dcd42e350d40381a9ca4d9e3b522))
* improve tui ux by adding command logs ([46185b6](https://github.com/tobias-walle/agency/commit/46185b6d1590a0c868fd37f778045930d4e3f406))
* launch configured agents inside daemon pty ([9f28dee](https://github.com/tobias-walle/agency/commit/9f28dee51c799391b607aab94ec005b4215b56b4))
* **logging:** add detailed attach I/O tracing in cli and daemon to diagnose missing input bytes ([bce2aed](https://github.com/tobias-walle/agency/commit/bce2aed5192eb9a2bf7faa773d93d80c616043bf))
* migrate from git2 to gix & git process ([0dbda04](https://github.com/tobias-walle/agency/commit/0dbda0479efc61d1ae12f7b3a449dd670110c9de))
* open editor on new tasks and inherit env and add --no-edit ([74ff653](https://github.com/tobias-walle/agency/commit/74ff6534ef367e5c4c955086bcd1871df21227a4))
* parse keybindings from config and use for detach in client ([092c523](https://github.com/tobias-walle/agency/commit/092c523a9af08e2ff0e06dc878af66c96ad653e1))
* phase 10 (rpc handling) ([2850cff](https://github.com/tobias-walle/agency/commit/2850cff3c8ca4a804f0b932f07abe9b90ae875d4))
* **ps:** list tasks with reusable table helper and readable tests ([1af9453](https://github.com/tobias-walle/agency/commit/1af9453a245cf87cb84e463a13c2615f4b9cb6cd))
* record base_branch and use slug title, strip header in attach ([e7bbf61](https://github.com/tobias-walle/agency/commit/e7bbf6121c25a94854bee59df58658c0dee06946))
* resolve duplicate slugs by auto-incrementing and attach by id ([6f7a9b5](https://github.com/tobias-walle/agency/commit/6f7a9b50dd907fbd4f0e9f86f1945f40ad15e968))
* route child I/O to TUI and switch terminal just in time ([00059eb](https://github.com/tobias-walle/agency/commit/00059eb121783b3ce9799bd50f713de5cf9338df))
* send full scrollback on attach to enable scrolling after reattach ([b84fda2](https://github.com/tobias-walle/agency/commit/b84fda22f0f2fe189abb9705736b43df2c28acc5))
* set tasks to stopped on exit ([9593973](https://github.com/tobias-walle/agency/commit/9593973a34a235871bfb094c08aed43c89e209cb))
* show [1] Tasks and [2] Command Log hints; enlarge log to 5 lines and add focus + scrolling ([db6e1c2](https://github.com/tobias-walle/agency/commit/db6e1c2e1b5a5038294f9b48e5951ccf2b06660e))
* show only task descriptions in editor ([4258037](https://github.com/tobias-walle/agency/commit/425803729370f62482ab1ac718bb901e0ece79bb))
* slugify slugs and keep TUI overlay on error; cancel new on empty editor ([566cc1f](https://github.com/tobias-walle/agency/commit/566cc1fc263c559cac21a3d0f7d1e87640a294d6))
* soft-reset view after detach and TUI to avoid leftover modes ([67e34c0](https://github.com/tobias-walle/agency/commit/67e34c023d5cf98ef2cfe34e8e000f7937cb8f53))
* support per-task agent selection with YAML front matter and reduce duplication ([f9dd081](https://github.com/tobias-walle/agency/commit/f9dd081c16ee050d47e8a20639dcb115b5a18217))
* switch detach shortcut to ctrl-q to avoid SIGINT conflicts ([40c078f](https://github.com/tobias-walle/agency/commit/40c078ff797716e8c5dade5f492dd0a0a822164c))
* **tui:** show gray loading status during delete to give immediate feedback ([cbcad15](https://github.com/tobias-walle/agency/commit/cbcad157bf3b71e8e41adcb54b87d7602b056ad9))
* unify command placeholder expansion for agents and bootstrap ([d0505e1](https://github.com/tobias-walle/agency/commit/d0505e1bb413461e4eb686d31f677147d9d75b62))
* update PLN-6 based on new implementation ([7a535c7](https://github.com/tobias-walle/agency/commit/7a535c72a93dd3e03cd4f20dd034592e93c197bf))
* wip ([a646037](https://github.com/tobias-walle/agency/commit/a646037114ad05f458b1c485ca9fea2aacf03259))


### Performance Improvements

* optimize dev build performance ([81de822](https://github.com/tobias-walle/agency/commit/81de8226e8587c35344cf9de40f4ce77e9f4c278))
