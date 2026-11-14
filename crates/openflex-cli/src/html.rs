//! Self-contained interactive HTML preview for `openflex layout --html`.
//!
//! The file embeds one SVG per animation frame plus a small amount of vanilla
//! JavaScript — no CDN, no bundler, no external asset. Opening it in any modern
//! browser gives a rotatable/tiltable isometric view (drag + wheel) or a
//! pan/zoom flat view, and play/pause controls when `--animate N` produced more
//! than one frame.
//!
//! Rotation is applied as a CSS 3D transform on the rendered SVG. That keeps
//! the page tiny and deterministic: the geometry comes from the same renderer
//! as `--svg`, while the browser handles the interaction.

use std::fmt::Write as _;

use openflex_model::Model;

use crate::layout::{self, Frame};

/// Which renderer feeds the HTML preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// Flat top-down layout: drag pans, wheel zooms.
    Flat,
    /// Isometric boxes: drag rotates/tilts, wheel zooms.
    Isometric,
}

/// Build a complete, self-contained HTML document.
pub fn to_html(
    model: &Model,
    width: u32,
    height: u32,
    projection: Projection,
    frames: u32,
) -> String {
    let frames = frames.max(1);
    let mut frame_markup = String::new();
    let mut progress_values = Vec::with_capacity(frames as usize);
    for index in 0..frames {
        let frame = if frames == 1 {
            None
        } else {
            Some(Frame {
                index,
                count: frames,
            })
        };
        let svg = match projection {
            Projection::Flat => layout::to_svg_flat(model, width, height, frame),
            Projection::Isometric => layout::to_svg_3d(model, width, height, frame),
        };
        // Embedded SVGs must not carry an XML declaration inside HTML.
        let svg = svg
            .split_once("<svg")
            .map(|(_, rest)| format!("<svg{rest}"))
            .unwrap_or(svg);
        let _ = writeln!(
            frame_markup,
            r#"<div class="frame" data-index="{index}"{style}>{svg}</div>"#,
            style = if index == 0 { "" } else { " hidden" }
        );
        progress_values.push(if frames == 1 {
            0.0
        } else {
            index as f64 / (frames - 1) as f64
        });
    }
    let frames_json = serde_json::to_string(&progress_values).unwrap_or_else(|_| "[]".into());
    let projection_name = match projection {
        Projection::Flat => "flat",
        Projection::Isometric => "isometric",
    };
    let interactive = match projection {
        Projection::Flat => "drag to pan · wheel to zoom",
        Projection::Isometric => "drag to rotate/tilt · wheel to zoom",
    };
    let play_controls = if frames > 1 {
        r#"<button id="play" type="button">Play</button>
      <span id="frame-label">frame 1 / FRAME_COUNT</span>"#
            .replace("FRAME_COUNT", &frames.to_string())
    } else {
        "<span class=\"muted\">single frame</span>".to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OpenFlex preview: {model_name}</title>
<style>
  :root {{ color-scheme: light; }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; font: 14px system-ui, sans-serif; color: #0f172a; background: #e2e8f0;
         display: flex; flex-direction: column; height: 100vh; }}
  header {{ padding: 12px 18px; background: #f8fafc; border-bottom: 1px solid #cbd5e1; }}
  header h1 {{ margin: 0; font-size: 16px; }}
  header p {{ margin: 4px 0 0; color: #64748b; font-size: 12px; }}
  #stage {{ flex: 1; overflow: hidden; display: grid; place-items: center; cursor: grab; }}
  #stage.dragging {{ cursor: grabbing; }}
  #scene {{ transform-origin: center; transition: transform 80ms linear; will-change: transform; }}
  .frame svg {{ display: block; max-width: 90vw; max-height: 72vh; height: auto; }}
  #controls {{ display: flex; gap: 10px; align-items: center; justify-content: center;
               padding: 10px; background: #f8fafc; border-top: 1px solid #cbd5e1; }}
  button {{ font: inherit; padding: 5px 14px; border-radius: 6px; border: 1px solid #334155;
            background: #ffffff; cursor: pointer; }}
  button:hover {{ background: #e2e8f0; }}
  .muted {{ color: #64748b; font-size: 12px; }}
</style>
</head>
<body data-projection="{projection_name}">
<header>
  <h1>OpenFlex {projection_name} preview: {model_name}</h1>
  <p>{objects} objects · {connections} connections · {interactive}</p>
</header>
<div id="stage"><div id="scene" data-projection="{projection_name}">
{frame_markup}</div></div>
<div id="controls">
{play_controls}
  <span class="muted">{interactive}</span>
</div>
<script type="application/json" id="frames">{frames_json}</script>
<script>
(() => {{
  const stage = document.getElementById('stage');
  const scene = document.getElementById('scene');
  const frames = Array.from(document.querySelectorAll('.frame'));
  const mode = document.body.dataset.projection;
  let yaw = 0, tilt = 0, zoom = 1, panX = 0, panY = 0;
  let dragging = false, lastX = 0, lastY = 0;
  const apply = () => {{
    if (mode === 'isometric') {{
      scene.style.transform = `perspective(1400px) rotateX(${{tilt}}deg) rotateZ(${{yaw}}deg) scale(${{zoom}})`;
    }} else {{
      scene.style.transform = `translate(${{panX}}px, ${{panY}}px) scale(${{zoom}})`;
    }}
  }};
  stage.addEventListener('pointerdown', event => {{
    dragging = true; lastX = event.clientX; lastY = event.clientY;
    stage.classList.add('dragging'); stage.setPointerCapture(event.pointerId);
  }});
  stage.addEventListener('pointermove', event => {{
    if (!dragging) return;
    const dx = event.clientX - lastX, dy = event.clientY - lastY;
    lastX = event.clientX; lastY = event.clientY;
    if (mode === 'isometric') {{
      yaw = (yaw + dx * 0.4) % 360;
      tilt = Math.max(-70, Math.min(70, tilt - dy * 0.3));
    }} else {{
      panX += dx; panY += dy;
    }}
    apply();
  }});
  const stop = () => {{ dragging = false; stage.classList.remove('dragging'); }};
  stage.addEventListener('pointerup', stop);
  stage.addEventListener('pointercancel', stop);
  stage.addEventListener('wheel', event => {{
    event.preventDefault();
    zoom = Math.max(0.3, Math.min(4, zoom * Math.exp(-event.deltaY * 0.001)));
    apply();
  }}, {{ passive: false }});

  const play = document.getElementById('play');
  const label = document.getElementById('frame-label');
  if (play) {{
    let current = 0, timer = null;
    const show = index => {{
      current = (index + frames.length) % frames.length;
      frames.forEach((frame, frameIndex) => {{ frame.hidden = frameIndex !== current; }});
      if (label) label.textContent = `frame ${{current + 1}} / ${{frames.length}}`;
    }};
    play.addEventListener('click', () => {{
      if (timer) {{
        clearInterval(timer); timer = null; play.textContent = 'Play';
      }} else {{
        timer = setInterval(() => show(current + 1), 450);
        play.textContent = 'Pause';
      }}
    }});
  }}
  apply();
}})();
</script>
</body>
</html>
"#,
        model_name = escape_html(&model.name),
        objects = model.objects.len(),
        connections = model.connections.len(),
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "html-demo",
              "objects": [
                { "name": "Source1", "kind": "source", "pos": [0, 0] },
                { "name": "Sink1", "kind": "sink", "pos": [10, 0] }
              ],
              "connections": [ { "from": "Source1", "to": "Sink1" } ]
            }"#,
        )
        .expect("fixture")
    }

    #[test]
    fn html_embeds_svg_frames_and_controls() {
        let html = to_html(&model(), 800, 600, Projection::Isometric, 3);
        assert!(html.contains("data-projection=\"isometric\""));
        assert!(html.contains("Source1") && html.contains("Sink1"));
        assert_eq!(html.matches("<script").count(), 2, "frames + behaviour");
        assert!(html.contains("type=\"application/json\" id=\"frames\""));
        assert!(html.contains("id=\"play\""));
        assert!(html.contains("frame 1 / 3"));
        assert!(!html.contains("https://cdn"));
    }

    #[test]
    fn flat_html_has_a_single_frame_and_no_play_button() {
        let html = to_html(&model(), 800, 600, Projection::Flat, 1);
        assert!(html.contains("data-projection=\"flat\""));
        assert!(!html.contains("id=\"play\""));
        assert_eq!(html.matches("class=\"frame\"").count(), 1);
    }
}
