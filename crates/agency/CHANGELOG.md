# Changelog

## [0.3.0](https://github.com/tobias-walle/agency/compare/v0.2.0...v0.3.0) (2025-11-11)


### Features

* improve git merge logs ([91ce632](https://github.com/tobias-walle/agency/commit/91ce632b6e74ab7bf88b2e3514a50d9a4e0138be))
* send full scrollback on attach to enable scrolling after reattach ([b0caa93](https://github.com/tobias-walle/agency/commit/b0caa930f261e05eda2b799645d0c9865a0cdbae))
* show [1] Tasks and [2] Command Log hints; enlarge log to 5 lines and add focus + scrolling ([f4530db](https://github.com/tobias-walle/agency/commit/f4530dbfb79da91619d6b443ae3fdc6545fa0078))
* slugify slugs and keep TUI overlay on error; cancel new on empty editor ([c332f81](https://github.com/tobias-walle/agency/commit/c332f812311bdc77404eae4d99601337ad6c3e70))
* **tui:** show gray loading status during delete to give immediate feedback ([50904f5](https://github.com/tobias-walle/agency/commit/50904f5c569aa9c10adb01457b4099d50d899c41))


### Bug Fixes

* **gc:** avoid deleting agency/* branches when a worktree exists and prune worktrees first ([17068f9](https://github.com/tobias-walle/agency/commit/17068f9efc29d06654cebb7ab3f62931786dc6c9))
* leave alternate screen during interactive to restore scrollback ([0323ae5](https://github.com/tobias-walle/agency/commit/0323ae5dae1fe31c8459534a00a8318cfa778970))

## [0.2.0](https://github.com/tobias-walle/agency/compare/v0.1.1...v0.2.0) (2025-11-11)


### Features

* add option to select agent in TUI ([bde32b7](https://github.com/tobias-walle/agency/commit/bde32b74c4f8ed49eb81fea6eab9e22d1bc4fe1d))
* add override config for socket path ([9f35a10](https://github.com/tobias-walle/agency/commit/9f35a10e8085637995b70ee0c5153c38a661f1b1))


### Bug Fixes

* avoid flickering in TUI then opening interactive program ([cc0b13d](https://github.com/tobias-walle/agency/commit/cc0b13d2921868b03006e707d68ae49217ad7e55))

## [0.1.1](https://github.com/tobias-walle/agency/compare/v0.1.0...v0.1.1) (2025-11-10)


### Bug Fixes

* always attach to prevent missing cursor issues ([44d8217](https://github.com/tobias-walle/agency/commit/44d8217df9f55f715f20bc0eb112645f9989cee8))
* don't copy folders if not included ([f6fc4d4](https://github.com/tobias-walle/agency/commit/f6fc4d4c172439e66f3a61a5c2392a430b9faa3d))

## 0.1.0 (2025-11-10)


### Features

* add .gitignore entries on `init` ([1538a98](https://github.com/tobias-walle/agency/commit/1538a983e394034a75b5d7e689211a3bcacff37a))
* add agency merge & agency open ([b17acfd](https://github.com/tobias-walle/agency/commit/b17acfd096e220f7e62b346589526c6d48c30e33))
* add bootstrap cmd to prepare worktrees and drop attach prepare-only ([de9812d](https://github.com/tobias-walle/agency/commit/de9812d8d064c47f3571ee3ec1703409ecd2f44d))
* add config loading ([947df9e](https://github.com/tobias-walle/agency/commit/947df9e191ed9ca1e54da0ea5cd3a1bad93faff4))
* add event-driven tui with subscribe and input overlay ([28ddcfb](https://github.com/tobias-walle/agency/commit/28ddcfbe0b55edb6d298695c462b563f6ce53e0b))
* add idle detection ([d3b598f](https://github.com/tobias-walle/agency/commit/d3b598f8afeac29c00e1b2f856c3cec036af8d36))
* add missing commands to cli ([d7ba45f](https://github.com/tobias-walle/agency/commit/d7ba45f5e679bfbefbfd87d2d8748beec16705d3))
* add multi session pty management ([9d3439f](https://github.com/tobias-walle/agency/commit/9d3439fb2758ee9e18b5f9bfe0be58e1212f95e2))
* add setup cmd support ([b86cca6](https://github.com/tobias-walle/agency/commit/b86cca6ce82c434c040696c90cf484f0bd4fad72))
* add TUI (phase 1) ([30da84d](https://github.com/tobias-walle/agency/commit/30da84d24e1e9100b7fe93d6b2a3e35ec3c6e3c5))
* add worktrees, branches and new CLI commands per PLN-3 with centralized task resolution and tests ([0564b0c](https://github.com/tobias-walle/agency/commit/0564b0cf6f024c47d5fdd138c6ea4ccb5ec3d668))
* copy ignored files & some folders on worktree creation ([df6ace1](https://github.com/tobias-walle/agency/commit/df6ace115495a214c19c03bd3b1f037dfaba8c3d))
* copy pty-demo and add attach/daemon to enable PTY sessions ([0f8145a](https://github.com/tobias-walle/agency/commit/0f8145a33ec12b33c2f031c7fc8cebc04f97b125))
* create setup, defaults and init command ([96a6c71](https://github.com/tobias-walle/agency/commit/96a6c71c87e51a1639d9c3dc24d7643d6ef27ec5))
* default to attaching new tasks with --no-attach opt-out ([f503367](https://github.com/tobias-walle/agency/commit/f503367d027fcd50dd2a1dd9785f31a5ea2833f1))
* enable task creation via new command with tty-aware output ([66b6b7a](https://github.com/tobias-walle/agency/commit/66b6b7af83a59492219107282580c6d60ee7965b))
* extend agency default toml ([2f4df14](https://github.com/tobias-walle/agency/commit/2f4df14737acb99f4e348ee3767bd04867ff051f))
* improve logging ([b056f52](https://github.com/tobias-walle/agency/commit/b056f520ed30a6ee6e200f389b23fa9199678798))
* improve tui UX and add Stopped state ([3953ab5](https://github.com/tobias-walle/agency/commit/3953ab5c297a69ed72e9fab589a4db272f5ce89c))
* improve tui ux by adding command logs ([71b67fc](https://github.com/tobias-walle/agency/commit/71b67fc298640f172986dee6d30a73074da12b42))
* migrate from git2 to gix & git process ([ac6d417](https://github.com/tobias-walle/agency/commit/ac6d41794904fcf699df57d4a6733d70e7b7e9c0))
* open editor on new tasks and inherit env and add --no-edit ([692f277](https://github.com/tobias-walle/agency/commit/692f2770b392fb18fc05f9a3f5f18473b00ecf5b))
* parse keybindings from config and use for detach in client ([e6dfdaa](https://github.com/tobias-walle/agency/commit/e6dfdaa10c400f44ad842fb7807833a6877739bb))
* **ps:** list tasks with reusable table helper and readable tests ([c765f42](https://github.com/tobias-walle/agency/commit/c765f42f0298f69525b8947aa6146c49b3bfe2a3))
* record base_branch and use slug title, strip header in attach ([b1e37ef](https://github.com/tobias-walle/agency/commit/b1e37efc0449fba5a3e91f1139fb5f0d923f1fa7))
* resolve duplicate slugs by auto-incrementing and attach by id ([a7fb6e0](https://github.com/tobias-walle/agency/commit/a7fb6e01c6359c512f1413708048bf6ca41dfa3f))
* route child I/O to TUI and switch terminal just in time ([5c21a85](https://github.com/tobias-walle/agency/commit/5c21a8525580acfe426dbfbc46432286571a3da9))
* show only task descriptions in editor ([2a5fcb3](https://github.com/tobias-walle/agency/commit/2a5fcb3f4b7ebcdd570a7ff3db2dd3850e4dc033))
* soft-reset view after detach and TUI to avoid leftover modes ([0526d82](https://github.com/tobias-walle/agency/commit/0526d826a970eb301d56489f274952ef309bae2f))
* support per-task agent selection with YAML front matter and reduce duplication ([d953138](https://github.com/tobias-walle/agency/commit/d95313848e1a1334d0639b621485f7e8bd7e81d4))
* switch detach shortcut to ctrl-q to avoid SIGINT conflicts ([8decfb8](https://github.com/tobias-walle/agency/commit/8decfb888331e4de76be954fa7b7a5d165193541))
* unify command placeholder expansion for agents and bootstrap ([fda9f16](https://github.com/tobias-walle/agency/commit/fda9f16a9a42ce7c707d872afa6e974f405c2a57))
* wip ([dcca483](https://github.com/tobias-walle/agency/commit/dcca48384f25dcf7091d031274d4ad34793280fc))


### Bug Fixes

* allow merge with unstaged changes ([45e8551](https://github.com/tobias-walle/agency/commit/45e855163d0deaed3979587235ba5e449965c226))
* allow stop at handshake and add ack with clearer logging to fix stop errors ([4431c97](https://github.com/tobias-walle/agency/commit/4431c97ee664757216ab87156ea6047f38937c81))
* centralize notification handling and fix bug that tui wasn't updated after merge ([0626194](https://github.com/tobias-walle/agency/commit/0626194ce97e926c610572819315de319db601ba))
* fix agency ps table formatting ([4500fa2](https://github.com/tobias-walle/agency/commit/4500fa24f46b223026a8782dcea191472bb7bdb8))
* fix cursor request handling with codex ([d110150](https://github.com/tobias-walle/agency/commit/d110150ddec25de87ce09cdbb1b2aacc1be2e593))
* fix idle detection and add logs ([a9776f7](https://github.com/tobias-walle/agency/commit/a9776f733eb6172ea6725c844401cbea52726916))
* fix scroll issue in pty ([61dafdc](https://github.com/tobias-walle/agency/commit/61dafdc45fddd543711bb44ff87dcc938c4e546b))
* fix tests ([8fa6fe0](https://github.com/tobias-walle/agency/commit/8fa6fe07a0ac3da3bce6e2073795b10ad57dc8fd))
* fix warnings ([127a2f7](https://github.com/tobias-walle/agency/commit/127a2f7615eac6e76a862dd9fab42b55bc5d40aa))
* improve error handling if daemon is stopped ([8baf449](https://github.com/tobias-walle/agency/commit/8baf449ae3ec8c43e840dd60fe478d1080b6d5de))
* make ctrl-q detach work with extended keyboard via termwiz ([15e93d9](https://github.com/tobias-walle/agency/commit/15e93d9034c134aa0c0f560de0ede4db9e375266))
* make enter reliably restart session and re-arm after each exit ([8c2b035](https://github.com/tobias-walle/agency/commit/8c2b03500ffdb8a39c722aaed95c143520d39d26))
* reattach to session on same task ([2732a57](https://github.com/tobias-walle/agency/commit/2732a57967d469424b6119e0ef4d4f2fcdf3997b))
* remove newline from frontmatter ([ee740de](https://github.com/tobias-walle/agency/commit/ee740de0e6543e3a932ee883954b4ea806e98e00))
* remove redudant tests ([8c2f039](https://github.com/tobias-walle/agency/commit/8c2f0392e3193aea34b1414c167625ac5b32f9fa))
* resolve clippy pedantic warnings and improve task markdown utils ([84dabc4](https://github.com/tobias-walle/agency/commit/84dabc458277f90eea6ac756a371389e0b1de78e))
* resolve pedantic clippy lints to make just fix green ([8aa2924](https://github.com/tobias-walle/agency/commit/8aa292473d425c6b4a61deaf1ca5b4455b394f4c))
* restore terminal state to avoid leftover extended keyboard modes ([63932a1](https://github.com/tobias-walle/agency/commit/63932a1bf94b3acd9c4ff69caf0ef054d200af46))
* revert workaround ([015c07f](https://github.com/tobias-walle/agency/commit/015c07f9d252d09df5adb07809027a7e52d890aa))
* track session status and broadcast after restart to update TUI ([e632a7b](https://github.com/tobias-walle/agency/commit/e632a7b8f52af887742a84074a13a1a96f198c61))
* update fs after merge ([c1e9ed5](https://github.com/tobias-walle/agency/commit/c1e9ed58cfab32d1b0c83251e013a908af8f5851))
