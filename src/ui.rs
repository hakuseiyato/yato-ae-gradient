//! Custom Comp UI — コンポ／レイヤーウィンドウ上のドラッグ描画。
//!
//! click: クリック位置を始点・終点にセットしドラッグ開始
//! drag : 終点を現在のマウス位置へ更新（last_time で終了）
//! draw : 始点→終点の線 + 両端ハンドルを drawbot で描画
//!
//! 座標変換は custom_comp_ui example を参照。マウスは frame(画面)座標で来るので
//! レイヤー座標へ変換して Point パラメータへ書き込み、描画時は逆変換する。

use super::*;

const HANDLE_SIZE: f32 = 7.0;
const REFCON_DRAWING: ae::sys::A_intptr_t = 1;

// ---- frame(画面)座標 → レイヤー座標(Fixed) ----
fn comp_frame_to_layer(
    in_data: &ae::InData,
    event: &mut ae::EventExtra,
    frame_pt: ae::Point,
) -> Result<ae::sys::PF_FixedPoint, Error> {
    let mut fix = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from_int(frame_pt.h).as_fixed(),
        y: ae::Fixed::from_int(frame_pt.v).as_fixed(),
    };
    event.callbacks().frame_to_source(&mut fix)?;
    event.callbacks().comp_to_layer(in_data.current_time(), in_data.time_scale(), &mut fix)?;
    Ok(fix)
}

fn layer_frame_to_layer(
    _in_data: &ae::InData,
    event: &mut ae::EventExtra,
    frame_pt: ae::Point,
) -> Result<ae::sys::PF_FixedPoint, Error> {
    let mut fix = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from_int(frame_pt.h).as_fixed(),
        y: ae::Fixed::from_int(frame_pt.v).as_fixed(),
    };
    event.callbacks().frame_to_source(&mut fix)?;
    Ok(fix)
}

// ---- レイヤー座標 → frame(画面)座標 ----
fn layer_to_comp_frame(
    in_data: &ae::InData,
    event: &mut ae::EventExtra,
    layer_pt: ae::Point,
    frame_pt: &mut ae::Point,
) -> Result<(), Error> {
    let mut fix = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from_int(layer_pt.h).as_fixed(),
        y: ae::Fixed::from_int(layer_pt.v).as_fixed(),
    };
    event.callbacks().layer_to_comp(in_data.current_time(), in_data.time_scale(), &mut fix)?;
    event.callbacks().source_to_frame(&mut fix)?;
    frame_pt.h = ae::Fixed::from_fixed(fix.x).to_int();
    frame_pt.v = ae::Fixed::from_fixed(fix.y).to_int();
    Ok(())
}

fn layer_to_layer_frame(
    _in_data: &ae::InData,
    event: &mut ae::EventExtra,
    layer_pt: ae::Point,
    frame_pt: &mut ae::Point,
) -> Result<(), Error> {
    let mut fix = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from_int(layer_pt.h).as_fixed(),
        y: ae::Fixed::from_int(layer_pt.v).as_fixed(),
    };
    event.callbacks().source_to_frame(&mut fix)?;
    frame_pt.h = ae::Fixed::from_fixed(fix.x).to_int();
    frame_pt.v = ae::Fixed::from_fixed(fix.y).to_int();
    Ok(())
}

// ---- 現在のマウス位置をレイヤー座標(f32)で返す ----
fn mouse_to_layer(in_data: &ae::InData, event: &mut ae::EventExtra) -> Result<(f32, f32), Error> {
    let pt = event.screen_point();
    let fix = if event.window_type() == ae::WindowType::Comp {
        comp_frame_to_layer(in_data, event, pt)?
    } else {
        layer_frame_to_layer(in_data, event, pt)?
    };
    Ok((
        ae::Fixed::from_fixed(fix.x).as_f32(),
        ae::Fixed::from_fixed(fix.y).as_f32(),
    ))
}

// ---- Point パラメータを更新し、CHANGED_VALUE を立てて確実に反映させる ----
//   ParamDef は drop 時に checkin_param され値が AE へ commit される。
//   set_value_changed() で変更フラグを明示し、ドラッグ反映が黙って失われる事故を防ぐ。
fn set_point(
    params: &mut ae::Parameters<Params>,
    which: Params,
    x: f32,
    y: f32,
) -> Result<(), Error> {
    let mut p = params.get_mut(which)?;
    p.as_point_mut()?.set_value((x, y));
    p.set_value_changed();
    Ok(())
}

// ---- 始点を基準に終点を 45° 刻みへスナップ（Shift 拘束用）----
fn snap_to_45(sx: f32, sy: f32, x: f32, y: f32) -> (f32, f32) {
    let dx = x - sx;
    let dy = y - sy;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1e-3 {
        return (x, y);
    }
    let step = std::f32::consts::FRAC_PI_4; // 45°
    let ang = (dy.atan2(dx) / step).round() * step;
    (sx + dist * ang.cos(), sy + dist * ang.sin())
}

pub fn click(
    in_data: &ae::InData,
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
) -> Result<(), Error> {
    let wt = event.window_type();
    if wt == ae::WindowType::Comp || wt == ae::WindowType::Layer {
        let (lx, ly) = mouse_to_layer(in_data, event)?;
        // 始点・終点をクリック位置に（ここから引き始める。ドラッグ前は始点=終点で素通し）
        set_point(params, Params::StartPoint, lx, ly)?;
        set_point(params, Params::EndPoint, lx, ly)?;

        event.set_send_drag(true);
        event.set_continue_refcon(0, REFCON_DRAWING);
        event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
    }
    Ok(())
}

pub fn drag(
    in_data: &ae::InData,
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
) -> Result<(), Error> {
    if event.continue_refcon(0) == REFCON_DRAWING {
        let (mut lx, mut ly) = mouse_to_layer(in_data, event)?;

        // Shift 押下中は始点基準で 45° 刻みに拘束（PS のグラデツール相当）
        if event.modifiers().contains(ae::Modifiers::SHIFT_KEY) {
            let start = params.get(Params::StartPoint)?.as_point()?.value();
            let (sx, sy) = snap_to_45(start.0, start.1, lx, ly);
            lx = sx;
            ly = sy;
        }

        set_point(params, Params::EndPoint, lx, ly)?;

        event.set_send_drag(true);
        event.set_continue_refcon(0, REFCON_DRAWING);

        if event.last_time() {
            event.set_continue_refcon(0, 0);
            event.set_send_drag(false);
        }
        event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
    }
    Ok(())
}

pub fn draw(
    in_data: &ae::InData,
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
) -> Result<(), Error> {
    let wt = event.window_type();
    if wt != ae::WindowType::Comp && wt != ae::WindowType::Layer {
        return Ok(());
    }

    let start = params.get(Params::StartPoint)?.as_point()?.value();
    let end = params.get(Params::EndPoint)?.as_point()?.value();

    let s_layer = ae::Point { h: start.0.round() as i32, v: start.1.round() as i32 };
    let e_layer = ae::Point { h: end.0.round() as i32, v: end.1.round() as i32 };
    let mut s_frame = ae::Point { h: 0, v: 0 };
    let mut e_frame = ae::Point { h: 0, v: 0 };
    if wt == ae::WindowType::Comp {
        layer_to_comp_frame(in_data, event, s_layer, &mut s_frame)?;
        layer_to_comp_frame(in_data, event, e_layer, &mut e_frame)?;
    } else {
        layer_to_layer_frame(in_data, event, s_layer, &mut s_frame)?;
        layer_to_layer_frame(in_data, event, e_layer, &mut e_frame)?;
    }

    let drawbot = event.context_handle().drawing_reference()?;
    let supplier = drawbot.supplier()?;
    let surface = drawbot.surface()?;

    // テーマ（前景色＋ストローク）は Premiere では未対応なので分岐
    let theme = if !in_data.is_premiere() {
        Some(ae::pf::suites::EffectCustomUIOverlayTheme::new()?)
    } else {
        None
    };
    let color = match &theme {
        Some(t) => t.preferred_foreground_color()?,
        None => ae::drawbot::ColorRgba { red: 0.9, green: 0.9, blue: 0.9, alpha: 1.0 },
    };

    // 始点→終点の線（Radial でも中心→半径の指示線として有効）
    let mut path = supplier.new_path()?;
    path.move_to(s_frame.h as f32, s_frame.v as f32)?;
    path.line_to(e_frame.h as f32, e_frame.v as f32)?;
    match &theme {
        Some(t) => t.stroke_path(&drawbot, &path, false)?,
        None => {
            let pen = supplier.new_pen(&color, 1.5)?;
            surface.stroke_path(&pen, &path)?;
        }
    }

    // 両端ハンドル
    for p in [s_frame, e_frame] {
        let bbox = ae::drawbot::RectF32 {
            left: p.h as f32 - HANDLE_SIZE / 2.0,
            top: p.v as f32 - HANDLE_SIZE / 2.0,
            width: HANDLE_SIZE,
            height: HANDLE_SIZE,
        };
        surface.paint_rect(&color, &bbox)?;
    }

    // 有効な中間stop を線上にマーカー表示（位置の把握用・少し小さめ）
    let m = HANDLE_SIZE * 0.75;
    for (en, _col, pos) in MID_STOPS.iter() {
        if params.get(*en)?.as_checkbox()?.value() {
            let p = (params.get(*pos)?.as_float_slider()?.value() as f32 / 100.0).clamp(0.0, 1.0);
            let mx = s_frame.h as f32 + (e_frame.h - s_frame.h) as f32 * p;
            let my = s_frame.v as f32 + (e_frame.v - s_frame.v) as f32 * p;
            let bbox = ae::drawbot::RectF32 {
                left: mx - m / 2.0,
                top: my - m / 2.0,
                width: m,
                height: m,
            };
            surface.paint_rect(&color, &bbox)?;
        }
    }

    event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
    Ok(())
}
