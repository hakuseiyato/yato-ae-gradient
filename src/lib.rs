//! Yato Gradient — After Effects ネイティブエフェクト（PoC）
//!
//! コンポ画面上を **ドラッグして「ここからここ」** とグラデーション線を引く。
//! 多色（多stop）の Linear / Radial グラデーションをレイヤーに描画する。
//!
//! - 幾何: Start/End ポイント（軸）、Style(Linear/Radial)
//! - 色: Start/End カラー + 任意の中間stop ×3（有効/色/位置）。位置でソートして区間補間。
//! - Custom Comp UI: Click→Drag→Draw でドラッグ描画（`ui` モジュール）。Shift で 45° 拘束。
//!
//! AE 標準パラメータには「グラデーションstop配列」が無い（arbitrary 不可・NO_VALUE）ため、
//! 固定数の stop を個別パラメータで持ち、レンダー時に有効stopを集めてソート→区間補間する。
//!
//! 移植元: virtualritz/after-effects `examples/custom_comp_ui`（楕円→グラデへ置換）

// 以下 2 つは `ae::define_effect!` マクロ展開内で出る clippy 警告。
// マクロ内部の話でこちら側では修正できないため crate 全体で許可しておく。
#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;

mod ui;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Style,
    StartPoint,
    EndPoint,
    StartColor,
    EndColor,
    MidsStart,
    Mid1Enable,
    Mid1Color,
    Mid1Pos,
    Mid2Enable,
    Mid2Color,
    Mid2Pos,
    Mid3Enable,
    Mid3Color,
    Mid3Pos,
    MidsEnd,
    Reverse,
    Repeat,
    Mirror,
    Dither,
    Opacity,
}

// Style ポップアップの値（1 基点インデックス）
const STYLE_LINEAR: i32 = 1;
const STYLE_RADIAL: i32 = 2;
const STYLE_ANGULAR: i32 = 3;
const STYLE_REFLECTED: i32 = 4;
const STYLE_DIAMOND: i32 = 5;

// 中間stop（有効チェック, 色, 位置）の組
const MID_STOPS: [(Params, Params, Params); 3] = [
    (Params::Mid1Enable, Params::Mid1Color, Params::Mid1Pos),
    (Params::Mid2Enable, Params::Mid2Color, Params::Mid2Pos),
    (Params::Mid3Enable, Params::Mid3Color, Params::Mid3Pos),
];

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

// ---- 色ユーティリティ ----
fn rgb_of(p: ae::Pixel8) -> [f32; 3] {
    [p.red as f32 / 255.0, p.green as f32 / 255.0, p.blue as f32 / 255.0]
}
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

// 位置順にソート済みの stop 列から t の色をサンプル（区間線形補間・端はクランプ）
fn sample_stops(stops: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    if stops.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    let last = stops.len() - 1;
    if t >= stops[last].0 {
        return stops[last].1;
    }
    for j in 0..last {
        let (p0, c0) = stops[j];
        let (p1, c1) = stops[j + 1];
        if t <= p1 {
            let span = (p1 - p0).max(1e-6);
            return lerp3(c0, c1, (t - p0) / span);
        }
    }
    stops[last].1
}

// 8bit へ
fn to_u8(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}
// 16bit へ
fn to_u16(v: f32) -> u16 {
    (v * ae::MAX_CHANNEL16 as f32)
        .round()
        .clamp(0.0, ae::MAX_CHANNEL16 as f32) as u16
}

// 画素位置から決定的な疑似乱数 [0,1)（ディザ用）
fn hash01(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h as f32 / u32::MAX as f32
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(Params::Style, "Style", ae::PopupDef::setup(|f| {
            f.set_options(&["Linear", "Radial", "Angular", "Reflected", "Diamond"]);
            f.set_default(STYLE_LINEAR);
            f.set_value(f.default());
        }))?;

        params.add(Params::Reverse, "Reverse", ae::CheckBoxDef::setup(|f| {
            f.set_default(false);
            f.set_label("色順を反転");
            f.set_value(false);
        }))?;

        params.add(Params::Repeat, "Repeat", ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(1.0);
            f.set_valid_max(50.0);
            f.set_slider_min(1.0);
            f.set_slider_max(10.0);
            f.set_default(1.0);
            f.set_value(f.default());
            f.set_precision(0);
        }))?;

        params.add(Params::Mirror, "Repeat Mirror", ae::CheckBoxDef::setup(|f| {
            f.set_default(false);
            f.set_label("折り返し (ping-pong)");
            f.set_value(false);
        }))?;

        params.add(Params::Dither, "Dither", ae::CheckBoxDef::setup(|f| {
            f.set_default(true);
            f.set_label("バンディング低減");
            f.set_value(true);
        }))?;

        params.add(Params::Opacity, "Opacity", ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(0.0);
            f.set_valid_max(100.0);
            f.set_slider_min(0.0);
            f.set_slider_max(100.0);
            f.set_default(100.0);
            f.set_value(f.default());
            f.set_precision(1);
            f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
        }))?;

        params.add(Params::StartColor, "Start Color", ae::ColorDef::setup(|f| {
            f.set_default(ae::Pixel8 { red: 0, green: 0, blue: 0, alpha: 255 });
            f.set_value(f.default());
        }))?;

        params.add(Params::EndColor, "End Color", ae::ColorDef::setup(|f| {
            f.set_default(ae::Pixel8 { red: 255, green: 255, blue: 255, alpha: 255 });
            f.set_value(f.default());
        }))?;

        // Point のデフォルトはレイヤー寸法に対する %（原点は左上）
        params.add(Params::StartPoint, "Start", ae::PointDef::setup(|f| {
            f.set_restrict_bounds(false);
            f.set_default((10.0, 50.0));
            f.set_value(f.default());
        }))?;

        params.add(Params::EndPoint, "End", ae::PointDef::setup(|f| {
            f.set_restrict_bounds(false);
            f.set_default((90.0, 50.0));
            f.set_value(f.default());
        }))?;

        // 中間stop群（折りたたみグループ）
        let mid_defaults: [(f32, ae::Pixel8); 3] = [
            (25.0, ae::Pixel8 { red: 255, green: 0, blue: 0, alpha: 255 }),
            (50.0, ae::Pixel8 { red: 0, green: 255, blue: 0, alpha: 255 }),
            (75.0, ae::Pixel8 { red: 0, green: 0, blue: 255, alpha: 255 }),
        ];
        params.add_group(Params::MidsStart, Params::MidsEnd, "Mid Stops", true, |params| {
            for (i, (en, col, pos)) in MID_STOPS.iter().enumerate() {
                let (dpos, dcol) = mid_defaults[i];
                let n = i + 1;
                params.add(*en, &format!("Mid {n}"), ae::CheckBoxDef::setup(|f| {
                    f.set_default(false);
                    f.set_label("enable");
                    f.set_value(false);
                }))?;
                params.add(*col, &format!("Mid {n} Color"), ae::ColorDef::setup(|f| {
                    f.set_default(dcol);
                    f.set_value(f.default());
                }))?;
                params.add(*pos, &format!("Mid {n} Pos"), ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_default(dpos as f64);
                    f.set_value(f.default());
                    f.set_precision(1);
                    f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
                }))?;
            }
            Ok(())
        })?;

        // コンポ／レイヤーウィンドウのカスタム UI イベントを受け取る
        in_data.interact().register_ui(
            CustomUIInfo::new()
                .events(ae::CustomEventFlags::LAYER | ae::CustomEventFlags::COMP),
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "Yato Gradient, v1.0\rコンポ上をドラッグして多色グラデーションを引く。\r",
                );
            }

            ae::Command::Event { mut extra } => match extra.event() {
                ae::Event::Click(_) => {
                    if extra.send_drag() {
                        ui::drag(&in_data, params, &mut extra)?;
                    } else {
                        ui::click(&in_data, params, &mut extra)?;
                    }
                }
                ae::Event::Drag(_) => {
                    ui::drag(&in_data, params, &mut extra)?;
                }
                ae::Event::Draw(_) => {
                    ui::draw(&in_data, params, &mut extra)?;
                }
                _ => {}
            },

            ae::Command::Render { in_layer, mut out_layer } => {
                let out_extent = out_layer.extent_hint();

                let style = params.get(Params::Style)?.as_popup()?.value();
                let dither = params.get(Params::Dither)?.as_checkbox()?.value();
                let reverse = params.get(Params::Reverse)?.as_checkbox()?.value();
                let cycles = (params.get(Params::Repeat)?.as_float_slider()?.value() as f32).max(1.0);
                let mirror = params.get(Params::Mirror)?.as_checkbox()?.value();
                let opacity = (params.get(Params::Opacity)?.as_float_slider()?.value() as f32 / 100.0)
                    .clamp(0.0, 1.0);
                let start = params.get(Params::StartPoint)?.as_point()?.value();
                let end = params.get(Params::EndPoint)?.as_point()?.value();

                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let len2 = dx * dx + dy * dy;

                // 始点≒終点（クリックのみ等）は塗らずに素通し（非破壊）
                if len2 < 1e-6 {
                    if in_data.quality() == ae::Quality::Hi && !in_data.is_premiere() {
                        ae::pf::suites::WorldTransform::new()?
                            .copy_hq(in_data.effect_ref(), in_layer, out_layer, None, None)?;
                    } else if !in_data.is_premiere() {
                        ae::pf::suites::WorldTransform::new()?
                            .copy(in_data.effect_ref(), in_layer, out_layer, None, None)?;
                    } else {
                        out_layer.copy_from(&in_layer, None, None)?;
                    }
                    return Ok(());
                }

                // stop 列を構築（Start@0 + 有効な中間stop + End@1）→ 位置でソート
                let mut stops: Vec<(f32, [f32; 3])> = Vec::with_capacity(5);
                stops.push((0.0, rgb_of(params.get(Params::StartColor)?.as_color()?.value())));
                for (en, col, pos) in MID_STOPS.iter() {
                    if params.get(*en)?.as_checkbox()?.value() {
                        let p = (params.get(*pos)?.as_float_slider()?.value() as f32 / 100.0)
                            .clamp(0.0, 1.0);
                        stops.push((p, rgb_of(params.get(*col)?.as_color()?.value())));
                    }
                }
                stops.push((1.0, rgb_of(params.get(Params::EndColor)?.as_color()?.value())));
                stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                let len = len2.sqrt();
                let dsx_inv = f32::from(in_data.downsample_x().inv());
                let dsy_inv = f32::from(in_data.downsample_y().inv());

                let axis_ang = dy.atan2(dx);

                // ピクセル位置 (x,y) のグラデーション正規化位置 t ∈ [0,1]
                let calc_t = |x: i32, y: i32| -> f32 {
                    let px = x as f32 * dsx_inv;
                    let py = y as f32 * dsy_inv;
                    let rx = px - start.0;
                    let ry = py - start.1;
                    let t = match style {
                        STYLE_RADIAL => (rx * rx + ry * ry).sqrt() / len,
                        STYLE_ANGULAR => {
                            // 始点まわりの角度を軸方向基準で 0..1 に正規化
                            let mut a = (ry.atan2(rx) - axis_ang) / std::f32::consts::TAU;
                            a -= a.floor();
                            a
                        }
                        STYLE_REFLECTED => ((rx * dx + ry * dy) / len2).abs(),
                        STYLE_DIAMOND => {
                            let a = (rx * dx + ry * dy) / len2; // 軸方向
                            let b = (-rx * dy + ry * dx) / len2; // 軸直交方向
                            a.abs() + b.abs()
                        }
                        _ => (rx * dx + ry * dy) / len2, // Linear
                    };
                    let mut t = t.clamp(0.0, 1.0);
                    if reverse {
                        t = 1.0 - t;
                    }
                    // 繰り返し（mirror なら折り返し）
                    if cycles > 1.0 {
                        let s = t * cycles;
                        t = if mirror {
                            let m = s % 2.0;
                            if m > 1.0 { 2.0 - m } else { m }
                        } else {
                            s.fract()
                        };
                    }
                    t.clamp(0.0, 1.0)
                };

                in_layer.iterate_with(
                    &mut out_layer,
                    0,
                    out_extent.height(),
                    Some(out_extent),
                    |x: i32, y: i32, pixel: ae::GenericPixel, out_pixel: ae::GenericPixelMut| -> Result<(), Error> {
                        let grad = sample_stops(&stops, calc_t(x, y));
                        match (pixel, out_pixel) {
                            (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                                // Opacity で元画像とブレンド
                                let inp = [
                                    pixel.red as f32 / 255.0,
                                    pixel.green as f32 / 255.0,
                                    pixel.blue as f32 / 255.0,
                                ];
                                let o = lerp3(inp, grad, opacity);
                                // 8bit はバンディングが見えるので任意でディザ（±0.5 LSB の決定的ノイズ）
                                let n = if dither { (hash01(x, y) - 0.5) / 255.0 } else { 0.0 };
                                *out_pixel = ae::Pixel8 {
                                    alpha: pixel.alpha,
                                    red: to_u8(o[0] + n),
                                    green: to_u8(o[1] + n),
                                    blue: to_u8(o[2] + n),
                                };
                            }
                            (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                                let mx = ae::MAX_CHANNEL16 as f32;
                                let inp = [
                                    pixel.red as f32 / mx,
                                    pixel.green as f32 / mx,
                                    pixel.blue as f32 / mx,
                                ];
                                let o = lerp3(inp, grad, opacity);
                                *out_pixel = ae::Pixel16 {
                                    alpha: pixel.alpha,
                                    red: to_u16(o[0]),
                                    green: to_u16(o[1]),
                                    blue: to_u16(o[2]),
                                };
                            }
                            (ae::GenericPixel::PixelF32(pixel), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                                let inp = [pixel.red, pixel.green, pixel.blue];
                                let o = lerp3(inp, grad, opacity);
                                *out_pixel = ae::PixelF32 {
                                    alpha: pixel.alpha,
                                    red: o[0],
                                    green: o[1],
                                    blue: o[2],
                                };
                            }
                            _ => return Err(Error::BadCallbackParameter),
                        }
                        Ok(())
                    },
                )?;
            }

            _ => {}
        }
        Ok(())
    }
}
