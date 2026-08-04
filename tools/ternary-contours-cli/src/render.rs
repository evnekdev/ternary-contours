use std::{error::Error, fs, path::Path};

use plotters::{
    coord::Shift,
    drawing::{DrawingArea, IntoDrawingArea},
    prelude::*,
};
use plotters_ternary::{
    MarkerShape, TernaryChartBuilder, TernaryLineSeries, TernaryPoint, TernaryPointSeries,
    TernaryStableContourSeries,
};
use ternary_contours::StablePhaseId;

use crate::{LiquidusProjection, TabulatedTernaryDataset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Svg,
    Png,
}

impl OutputFormat {
    pub fn from_path(path: &Path) -> Result<Self, RenderError> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("svg") => Ok(Self::Svg),
            Some("png") => Ok(Self::Png),
            _ => Err(RenderError::UnsupportedOutput(path.display().to_string())),
        }
    }
}
/// Which stable-boundary paths the renderer should show.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderPathMode {
    Raw,
    #[default]
    Regularized,
    Overlay,
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub title: Option<String>,
    pub show_isotherms: bool,
    pub show_univariants: bool,
    pub show_invariants: bool,
    pub show_binary_invariants: bool,
    pub show_grid: bool,
    pub show_samples: bool,
    pub show_labels: bool,
    pub show_corner_labels: bool,
    pub show_legend: bool,
    pub marker_size: u32,
    pub line_width: u32,
    pub path_mode: RenderPathMode,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 1_200,
            height: 950,
            title: None,
            show_isotherms: true,
            show_univariants: true,
            show_invariants: true,
            show_binary_invariants: true,
            show_grid: false,
            show_samples: false,
            show_labels: true,
            show_corner_labels: true,
            show_legend: true,
            marker_size: 8,
            line_width: 3,
            path_mode: RenderPathMode::Regularized,
        }
    }
}

/// An opaque RGBA image produced by the shared Plotters renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("output file must use a .svg or .png extension: {0}")]
    UnsupportedOutput(String),
    #[error("render dimensions must be positive and fit in memory: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("could not create output directory: {0}")]
    CreateDirectory(#[from] std::io::Error),
    #[error("rendering failed: {0}")]
    Draw(String),
}

/// Render a neutral projection to SVG or PNG according to the output extension.
pub fn render_to_path(
    path: impl AsRef<Path>,
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    options: &RenderOptions,
    format: Option<OutputFormat>,
) -> Result<(), RenderError> {
    render_to_path_with_raw(path, dataset, projection, None, options, format)
}

/// Export using the same renderer while optionally retaining raw paths for an overlay.
pub fn render_to_path_with_raw(
    path: impl AsRef<Path>,
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    raw_projection: Option<&LiquidusProjection>,
    options: &RenderOptions,
    format: Option<OutputFormat>,
) -> Result<(), RenderError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    match format.unwrap_or(OutputFormat::from_path(path)?) {
        OutputFormat::Svg => {
            let root = SVGBackend::new(path, (options.width, options.height)).into_drawing_area();
            render(&root, dataset, projection, raw_projection, options)
                .map_err(|error| RenderError::Draw(error.to_string()))?;
            root.present()
                .map_err(|error| RenderError::Draw(error.to_string()))
        }
        OutputFormat::Png => {
            let root =
                BitMapBackend::new(path, (options.width, options.height)).into_drawing_area();
            render(&root, dataset, projection, raw_projection, options)
                .map_err(|error| RenderError::Draw(error.to_string()))?;
            root.present()
                .map_err(|error| RenderError::Draw(error.to_string()))
        }
    }
}

/// Render the same static plot into an RGBA bitmap suitable for a native texture.
///
/// Plotters' in-memory bitmap backend writes RGB bytes in channel order. The
/// conversion is explicit so callers never need to depend on a backend-specific
/// RGB/BGR assumption.
pub fn render_to_bitmap(
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    options: &RenderOptions,
) -> Result<RenderedBitmap, RenderError> {
    render_to_bitmap_with_raw(dataset, projection, None, options)
}

/// Render to an in-memory texture while optionally retaining raw paths for an overlay.
pub fn render_to_bitmap_with_raw(
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    raw_projection: Option<&LiquidusProjection>,
    options: &RenderOptions,
) -> Result<RenderedBitmap, RenderError> {
    let rgb_len = rgb_len(options.width, options.height)?;
    let mut rgb = vec![0; rgb_len];
    {
        let root = BitMapBackend::with_buffer(&mut rgb, (options.width, options.height))
            .into_drawing_area();
        render(&root, dataset, projection, raw_projection, options)
            .map_err(|error| RenderError::Draw(error.to_string()))?;
        root.present()
            .map_err(|error| RenderError::Draw(error.to_string()))?;
    }
    Ok(RenderedBitmap {
        width: options.width,
        height: options.height,
        rgba: rgb_to_rgba(&rgb),
    })
}
fn rgb_len(width: u32, height: u32) -> Result<usize, RenderError> {
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(RenderError::InvalidDimensions { width, height })?;
    pixels
        .checked_mul(3)
        .filter(|_| width > 0 && height > 0)
        .ok_or(RenderError::InvalidDimensions { width, height })
}

/// Convert Plotters' RGB memory buffer to egui-compatible, opaque RGBA bytes.
pub fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    debug_assert_eq!(rgb.len() % 3, 0);
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], u8::MAX]);
    }
    rgba
}

fn render<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    raw_projection: Option<&LiquidusProjection>,
    options: &RenderOptions,
) -> Result<(), Box<dyn Error>>
where
    DB::ErrorType: 'static,
{
    root.fill(&WHITE)?;
    let title = options
        .title
        .as_deref()
        .or(dataset.title.as_deref())
        .unwrap_or("Stable liquidus projection");
    let mut chart = TernaryChartBuilder::on(root)
        .caption(title, ("sans-serif", 24, FontStyle::Bold, &BLACK))
        .margin(30)
        .build()?;
    let mut mesh = chart.configure_mesh();
    if options.show_labels {
        mesh = mesh
            .axis_a_name(&dataset.components[0].name)
            .axis_b_name(&dataset.components[1].name)
            .axis_c_name(&dataset.components[2].name);
    } else {
        mesh = mesh.hide_axis_names();
    }
    if options.show_corner_labels {
        mesh = mesh
            .corner_a_name(corner_label(&dataset.components[0].name, "[1, 0, 0]"))
            .corner_b_name(corner_label(&dataset.components[1].name, "[0, 1, 0]"))
            .corner_c_name(corner_label(&dataset.components[2].name, "[0, 0, 1]"));
    } else {
        mesh = mesh.hide_corner_names();
    }
    mesh.draw()?;

    let phase_names = dataset
        .phases
        .iter()
        .map(|phase| (phase.id, phase.name.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    if options.show_isotherms {
        chart.draw_series(
            TernaryStableContourSeries::new(&projection.stable_contours)
                .style_by_phase(|phase| phase_style(phase, options.line_width))
                .legend(options.show_legend)
                .phase_formatter(|phase| {
                    phase_names
                        .get(&phase)
                        .cloned()
                        .unwrap_or_else(|| format!("Phase {}", phase.0))
                }),
        )?;
    }
    if options.show_univariants {
        let raw = raw_projection.unwrap_or(projection);
        let paths = match options.path_mode {
            RenderPathMode::Raw => {
                vec![(raw, RGBColor(208, 111, 29).stroke_width(options.line_width))]
            }
            RenderPathMode::Regularized => {
                vec![(projection, BLACK.mix(0.72).stroke_width(options.line_width))]
            }
            RenderPathMode::Overlay => vec![
                (raw, RGBColor(208, 111, 29).stroke_width(options.line_width)),
                (projection, BLACK.mix(0.72).stroke_width(options.line_width)),
            ],
        };
        for (source, style) in paths {
            for path in &source.stable_boundaries.univariants {
                chart.draw_series(TernaryLineSeries::new(
                    path.points
                        .iter()
                        .map(|point| TernaryPoint::from(point.as_array()))
                        .collect::<Vec<_>>(),
                    style,
                ))?;
            }
        }
    }
    if options.show_binary_invariants {
        let points = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Binary(_)))
            .map(|node| TernaryPoint::from(node.point().as_array()))
            .collect::<Vec<_>>();
        if !points.is_empty() {
            chart.draw_series(
                TernaryPointSeries::new(points)
                    .marker(MarkerShape::Diamond)
                    .size(options.marker_size)
                    .style(RGBColor(232, 125, 28).filled()),
            )?;
        }
    }
    if options.show_invariants {
        let points = projection
            .stable_boundaries
            .nodes
            .iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Interior(_)))
            .map(|node| TernaryPoint::from(node.point().as_array()))
            .collect::<Vec<_>>();
        if !points.is_empty() {
            chart.draw_series(
                TernaryPointSeries::new(points)
                    .marker(MarkerShape::Circle)
                    .size(options.marker_size.saturating_add(1))
                    .style(RGBColor(164, 48, 143).filled()),
            )?;
        }
    }
    if options.show_grid || options.show_samples {
        let points = dataset
            .grids
            .iter()
            .flat_map(|grid| grid.compositions().iter().copied())
            .map(TernaryPoint::from)
            .collect::<Vec<_>>();
        chart.draw_series(
            TernaryPointSeries::new(points)
                .marker(MarkerShape::Circle)
                .size(if options.show_grid {
                    3
                } else {
                    options.marker_size.min(6)
                })
                .style(BLACK.mix(0.35).filled()),
        )?;
    }
    if options.show_legend && options.show_isotherms {
        chart
            .configure_series_labels()
            .background_style(WHITE.mix(0.82))
            .border_style(BLACK)
            .draw()?;
    }
    Ok(())
}

fn corner_label(name: &str, composition: &str) -> String {
    format!("{name} {composition}")
}

fn phase_style(phase: StablePhaseId, line_width: u32) -> ShapeStyle {
    const PALETTE: [RGBColor; 8] = [
        RGBColor(31, 119, 180),
        RGBColor(214, 39, 40),
        RGBColor(44, 160, 44),
        RGBColor(148, 103, 189),
        RGBColor(255, 127, 14),
        RGBColor(23, 190, 207),
        RGBColor(140, 86, 75),
        RGBColor(227, 119, 194),
    ];
    PALETTE[phase.0 as usize % PALETTE.len()].stroke_width(line_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectionOptions, calculate_projection, parse_str};

    #[test]
    fn corner_labels_follow_semantic_composition_coordinates() {
        assert_eq!(corner_label("CaO", "[1, 0, 0]"), "CaO [1, 0, 0]");
        assert_eq!(corner_label("PbO", "[0, 1, 0]"), "PbO [0, 1, 0]");
        assert_eq!(corner_label("ZnO", "[0, 0, 1]"), "ZnO [0, 0, 1]");
    }

    #[test]
    fn rgb_to_rgba_preserves_channel_order_and_adds_opacity() {
        assert_eq!(
            rgb_to_rgba(&[0x12, 0x34, 0x56, 0xab, 0xcd, 0xef]),
            vec![0x12, 0x34, 0x56, 0xff, 0xab, 0xcd, 0xef, 0xff]
        );
    }

    #[test]
    fn plotters_memory_backend_uses_rgb_channel_order() {
        let mut rgb = vec![0; 3];
        {
            let root = BitMapBackend::with_buffer(&mut rgb, (1, 1)).into_drawing_area();
            root.fill(&RGBColor(0x12, 0x34, 0x56)).unwrap();
            root.present().unwrap();
        }
        assert_eq!(rgb, [0x12, 0x34, 0x56]);
        assert_eq!(rgb_to_rgba(&rgb), [0x12, 0x34, 0x56, 0xff]);
    }
    #[test]
    fn bitmap_render_has_requested_dimensions_and_plot_pixels() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        let bitmap = render_to_bitmap(
            &dataset,
            &projection,
            &RenderOptions {
                width: 360,
                height: 300,
                ..RenderOptions::default()
            },
        )
        .unwrap();

        assert_eq!((bitmap.width, bitmap.height), (360, 300));
        assert_eq!(bitmap.rgba.len(), 360 * 300 * 4);
        assert!(
            bitmap
                .rgba
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [255, 255, 255])
        );
    }

    #[test]
    fn zero_bitmap_dimension_is_rejected() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        assert!(matches!(
            render_to_bitmap(
                &dataset,
                &projection,
                &RenderOptions {
                    width: 0,
                    ..RenderOptions::default()
                }
            ),
            Err(RenderError::InvalidDimensions { .. })
        ));
    }
}
