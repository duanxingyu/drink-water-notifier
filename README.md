# 润滴

工作日喝水提醒。到了你设定的时段，它会弹出一个始终置顶的窗口，杯子里的水会晃一晃，并说「该喝水啦」。窗口可以点掉，也可以记下一杯，或者五分钟后再响。平时它只留在系统托盘 / 菜单栏里。

调度规则在 Rust 里，不依赖界面，可以用 `cargo test` 单独跑。界面是 Tauri 2：Rust 负责时间、托盘、声音和开机启动，网页前端负责动画。

## 功能

- 默认只在周一到周五、09:30–18:30 提醒，间隔 30 分钟。工作日、起止时间和间隔（1–240 分钟）都可以改。
- 配置写在系统配置目录的 `config.toml`，杯数和暂停状态写在旁边的 `state.json`。
- 提醒窗口无边框、背景透明、始终置顶。到点播放一声自己生成的水滴声。可关声音、调音量。
- 「喝了」给今天加一杯。「稍后提醒」按设置里的分钟数再叫一次（默认 5 分钟），即使已经过了下班时间也会响这一次。点空白处、按 Esc，或倒计时结束，都只是关掉，不加杯。
- 托盘菜单：打开设置、立即提醒、暂停 1 小时、今日暂停、继续提醒、退出。
- 可选开机启动。

## 环境

- Rust stable（1.90 或更新，Tauri 2.12 的要求）
- Node.js 22
- Windows：WebView2 运行时。Windows 11 一般已经带了；没有的话安装包会尝试下载引导程序。
- macOS：Xcode 命令行工具，系统 10.15 或更新。
- 只在 Linux 上编译和测试时，还需要 WebKitGTK 4.1 和 ALSA 开发包：

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf \
  libssl-dev libasound2-dev pkg-config libayatana-appindicator3-dev
```

## 开发运行

在仓库根目录：

```bash
npm ci
npm run tauri dev
```

第一次会编译一会儿。窗口默认不自动打开，去系统托盘或菜单栏找水滴图标。第一次运行、配置文件还不存在时，会打开设置。

只看界面、不启桌面壳时：

```bash
npm run dev
```

浏览器打开 <http://127.0.0.1:43123/?view=reminder> 和 <http://127.0.0.1:43123/?view=settings>。这是同一套页面，方便预览动画；真正的倒计时、托盘和写配置要在 Tauri 里。

## 打包

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`。

### Windows

需要已安装 Visual Studio 生成工具，并勾选「使用 C++ 的桌面开发」。Rust 用 `rustup` 安装，目标是默认的 `x86_64-pc-windows-msvc`（ARM 自行加 `aarch64-pc-windows-msvc`）。

安装包是当前用户安装，不需要管理员。开机启动写的是当前用户的 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`，名字是 `Rundi`。

未签名的安装包可能被 SmartScreen 拦住，选「仍要运行」即可。分发给别人之前应再做代码签名。

### macOS

```bash
xcode-select --install
npm run tauri build
```

得到 `bundle/macos/Rundi.app`，以及 dmg。本仓库的构建配置没有签名身份，也没有公证。从浏览器下载后，系统会提示「无法验证开发者」。可以在访达里右键应用，选「打开」，或执行：

```bash
xattr -dr com.apple.quarantine /Applications/Rundi.app
```

要对外分发，需要 Apple Developer 账号、Developer ID 签名，再用 `notarytool` 公证。开机启动用的是 `~/Library/LaunchAgents` 里的 Launch Agent，不需要辅助功能权限。这个应用不申请通知、麦克风或屏幕录制权限。

## 配置

程序把目录准备好。也可以自己建文件。

| 系统 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\rundi\config.toml` |
| macOS | `~/Library/Application Support/rundi/config.toml` |
| Linux | `~/.config/rundi/config.toml` |

同目录的 `state.json` 记录上次提醒时间、暂停、稍后，以及当天杯数。一般不用手改。

```toml
workdays = ["Mon", "Tue", "Wed", "Thu", "Fri"]
start = "09:30"
end = "18:30"
interval_minutes = 30
sound_enabled = true
volume = 0.7
auto_dismiss_seconds = 20
snooze_minutes = 5
launch_at_login = false
```

星期也可以写成 `周一` 或 `1`（1 是周一，7 是周日）。时间用 `HH:MM`。设置窗口里保存之后会重写这个文件。改完文件要重启应用，运行中的进程不会重新读盘；用窗口保存则会立刻生效。

## 调度规则

时间窗是半开区间 `[开始, 结束)`。09:30 会提醒，18:30 整不再提醒。

- 进入时间窗的第一下马上提醒，然后每过 `interval_minutes` 再提醒。间隔从「上一次真正弹出」起算。
- 若下一次正好落在结束时刻或更晚，就等到下一个工作日的开始。
- 周末或未勾选的日子不提醒。周五下班后，默认要到下周一 09:30。
- 暂停优先于一切。暂停 1 小时若跨过下班时间，醒来后如果已经不在时间窗里，就等到下一个工作日。今日暂停持续到本地时间的次日 0 点。
- 「稍后提醒」到点必响一次，哪怕已经离开时间窗。响过之后回到正常间隔。暂停会清掉还没到的稍后提醒。
- 启动时如果正好轮到该提醒（比如上次已经超过间隔），会马上弹一次。

这些边界都在 `crates/rundi-core` 的测试里。

## 托盘

左键或右键图标都可以打开菜单。

- 今日已喝 N 杯（只展示）
- 打开设置
- 立即提醒
- 暂停 1 小时 / 今日暂停；暂停中会变成「继续提醒」
- 退出

关掉设置窗口不会退出，菜单里写了「关掉这个窗口后，它还在托盘里」。

## 系统限制

**始终置顶**用的是系统提供的顶层窗口。普通窗口、大多数无边框全屏都能盖住。

- Windows：实现是 `HWND_TOPMOST`。独占全屏（例如很多游戏的独占模式）仍然会盖住提醒。无边框窗口化全屏通常可以。
- macOS：Tauri 默认的置顶是 `NSFloatingWindowLevel`，盖不住全屏空间。润滴在弹出时再把窗口抬到 `NSPopUpMenuWindowLevel`（101），并加上 `CanJoinAllSpaces` 和 `FullScreenAuxiliary`，让它尽量跟进全屏 Space。系统菜单栏、锁屏、以及部分独占全屏仍然更高。这是公开的 `NSWindow` API，不需要关闭 SIP，也不打开 Tauri 的 macOS 私有 API 开关。
- Linux：开发时可以跑。托盘依赖 StatusNotifier 或 Ayatana AppIndicator。GNOME 默认可能把托盘藏起来，需要扩展才看得见图标。

透明无边框窗口在三套系统上的阴影不一样。提醒的卡片阴影画在页面里，避免 Windows 给无边框窗口加一圈白边。

## 测试

```bash
cargo test -p rundi-core
cargo test --workspace
```

`rundi-core` 覆盖：上班前一分钟、09:30 整、18:30 整、间隔刚好到期和差一秒、周一全天 18 次、周五晚上到下周一、只选周二周四、只选周日、暂停跨过时间窗、今日暂停到午夜、稍后提醒在下班后仍响一次、空工作日不会空转。

提示音文件会在应用测试里被解码一次，确认 WAV 能被读出来。测试不会真的去放声音。

## 项目结构

```text
crates/rundi-core   日程、配置和杯数，无界面依赖
src-tauri            Tauri 壳：托盘、置顶窗口、声音、开机启动
src                  提醒动画和设置页
src-tauri/assets     自己生成的 drop.wav
tools/gen_assets.py  重新生成图标和提示音
```

图标和水滴声都是仓库里的脚本画出来的，没有第三方音效或图标授权问题。

## 持续集成

`.github/workflows/ci.yml` 在 Ubuntu 上安装 WebKit 依赖后跑 `cargo test` 和 `cargo check`。Windows 和 macOS 上同样跑测试，并用 `tauri-apps/tauri-action` 打安装包。macOS 产物未签名、未公证。
