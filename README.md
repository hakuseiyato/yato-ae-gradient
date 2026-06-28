# Yato Gradient（AE ネイティブエフェクト・PoC）

After Effects のコンポ画面上を **ドラッグして「ここからここ」** とグラデーション線を引く
ネイティブエフェクト。Rust + AE SDK（`virtualritz/after-effects` クレート）で実装。

## これは何の PoC か

AE 標準の Gradient Ramp は数値で始点/終点を指定する。これを **Photoshop のグラデツールのように
1 ドラッグで方向を引く** UX に置き換える検証版。ScriptUI ではビューポートのドラッグを拾えないため、
ネイティブプラグインが必須だった経緯がある。

### 機能

- **ドラッグで方向指定** — コンポ上を一筆ドラッグで始点→終点。Shift で 45° 拘束。
- **5 種のスタイル** — Linear / Radial / Angular(円錐) / Reflected(反射) / Diamond(菱形)。
- **多色 stop** — Start/End + 任意の中間 stop ×3（有効・色・位置）。位置順にソートして区間補間。
- **Reverse / Repeat(＋Mirror 折り返し)** — 色順反転、グラデの繰り返し。
- **Dither** — 8bit のバンディング低減（決定的ノイズ）。
- **Opacity** — 元画像とのブレンド（オーバーレイ/ティント用途）。
- 8/16/32bit 対応、しきい外はクランプ、始点≒終点は非破壊パススルー。

次フェーズ: コンポ上での stop 追加/移動、グラデーションマップ、プリセット、GPU 化。

## ビルド & インストール

前提（このマシンでは確認済み）: Rust(msvc) / Visual Studio C++ Build Tools / AE 2024–2026。

```powershell
# 管理者 PowerShell（MediaCore への書き込みに管理者権限が必要）
cd C:\Work\Yato\Claude\yato-ae-gradient
powershell -ExecutionPolicy Bypass -File .\install.ps1          # debug
# powershell -ExecutionPolicy Bypass -File .\install.ps1 -Release  # release
```

`install.ps1` が `cargo build` →`target\debug\yato_ae_gradient.dll` を
`C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\YatoGradient.aex` にコピーする。

その後 **After Effects を再起動** → `Effects > Yato > Yato Gradient`。

## 使い方

1. 平面（ソリッド）レイヤーに Yato Gradient を適用。
2. コンポ上で **ドラッグ** → 始点→終点に線とハンドルが出て、Start→End 色のグラデで塗られる。
   - **Shift ドラッグ**: 角度を 45° 刻みに拘束。
   - **Style = Radial**: 始点を中心、|終点-始点| を半径とする放射状グラデ。
3. エフェクトコントロールで各パラメータを調整：
   - **Style** Linear/Radial/Angular/Reflected/Diamond
   - **Start/End Color**、**Mid Stops**（Mid1–3 の enable/色/位置%）
   - **Reverse**、**Repeat**（＋**Repeat Mirror**）、**Dither**、**Opacity**
   - Start/End ポイントのクロスヘアは個別ドラッグも可。

> 始点と終点がほぼ同じ（クリックのみ等）のときは塗りつぶさず元画像を素通しする（非破壊）。

ログは [DebugView](https://learn.microsoft.com/sysinternals/downloads/debugview)（debug ビルド）。

## 構成

| ファイル | 役割 |
|---|---|
| `Cargo.toml` | cdylib / `after-effects`・`pipl`（git rev 固定） |
| `build.rs` | PiPL（プラグイン定義）生成 |
| `src/lib.rs` | パラメータ定義・Render（多stop補間・5スタイル・dither・opacity） |
| `src/ui.rs` | Custom Comp UI（Click/Drag/Draw・座標変換） |
| `install.ps1` | dev install |

移植元: `virtualritz/after-effects` の `examples/custom_comp_ui`（rev `ffdff33`）。
