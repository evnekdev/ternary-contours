use std::{error::Error, fs, path::Path};

use plotters::{coord::Shift, drawing::{DrawingArea, IntoDrawingArea}, prelude::*};
use plotters_ternary::{
    MarkerShape, TernaryChartBuilder, TernaryLineSeries, TernaryPoint, TernaryPointSeries,
    TernaryStableContourSeries,
};
use ternary_contours::StablePhaseId;

use crate::{LiquidusProjection, TabulatedTernaryDataset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat { Svg, Png }

impl OutputFormat {
    pub fn from_path(path: &Path) -> Result<Self, RenderError> {
        match path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("svg") => Ok(Self::Svg),
            Some("png") => Ok(Self::Png),
            _ => Err(RenderError::UnsupportedOutput(path.display().to_string())),
        }
    }
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
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 1_200, height: 950, title: None, show_isotherms: true,
            show_univariants: true, show_invariants: true, show_binary_invariants: true,
            show_grid: false, show_samples: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("output file must use a .svg or .png extension: {0}")]
    UnsupportedOutput(String),
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
    let path = path.as_ref();
    if let Some(parent) = path.parent() && !parent.as_os_str().is_empty() { fs::create_dir_all(parent)?; }
    match format.unwrap_or(OutputFormat::from_path(path)?) {
        OutputFormat::Svg => {
            let root = SVGBackend::new(path, (options.width, options.height)).into_drawing_area();
            render(&root, dataset, projection, options).map_err(|error| RenderError::Draw(error.to_string()))?;
            root.present().map_err(|error| RenderError::Draw(error.to_string()))
        }
        OutputFormat::Png => {
            let root = BitMapBackend::new(path, (options.width, options.height)).into_drawing_area();
            render(&root, dataset, projection, options).map_err(|error| RenderError::Draw(error.to_string()))?;
            root.present().map_err(|error| RenderError::Draw(error.to_string()))
        }
    }
}

fn render<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    dataset: &TabulatedTernaryDataset,
    projection: &LiquidusProjection,
    options: &RenderOptions,
) -> Result<(), Box<dyn Error>>
where DB::ErrorType: 'static {
    root.fill(&WHITE)?;
    let title = options.title.as_deref().or(dataset.title.as_deref()).unwrap_or("Stable liquidus projection");
    let mut chart = TernaryChartBuilder::on(root)
        .caption(title, ("sans-serif", 24, FontStyle::Bold, &BLACK))
        .margin(30)
        .build()?;
    chart.configure_mesh()
        .axis_a_name(&dataset.components[0].name)
        .axis_b_name(&dataset.components[1].name)
        .axis_c_name(&dataset.components[2].name)
        .draw()?;

    let phase_names = dataset.phases.iter().map(|phase| (phase.id, phase.name.clone())).collect::<std::collections::BTreeMap<_, _>>();
    if options.show_isotherms {
        chart.draw_series(
            TernaryStableContourSeries::new(&projection.stable_contours)
                .style_by_phase(phase_style)
                .legend(true)
                .phase_formatter(|phase| phase_names.get(&phase).cloned().unwrap_or_else(|| format!("Phase {}", phase.0))),
        )?;
    }
    if options.show_univariants {
        for path in &projection.stable_boundaries.univariants {
            chart.draw_series(TernaryLineSeries::new(
                path.points.iter().map(|point| TernaryPoint::from(point.as_array())).collect::<Vec<_>>(),
                BLACK.mix(0.72).stroke_width(3),
            ))?;
        }
    }
    if options.show_binary_invariants {
        let points = projection.stable_boundaries.nodes.iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Binary(_)))
            .map(|node| TernaryPoint::from(node.point().as_array())).collect::<Vec<_>>();
        if !points.is_empty() {
            chart.draw_series(TernaryPointSeries::new(points).marker(MarkerShape::Diamond).size(8).style(RGBColor(232, 125, 28).filled()))?;
        }
    }
    if options.show_invariants {
        let points = projection.stable_boundaries.nodes.iter()
            .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Interior(_)))
            .map(|node| TernaryPoint::from(node.point().as_array())).collect::<Vec<_>>();
        if !points.is_empty() {
            chart.draw_series(TernaryPointSeries::new(points).marker(MarkerShape::Circle).size(9).style(RGBColor(164, 48, 143).filled()))?;
        }
    }
    if options.show_grid || options.show_samples {
        let points = dataset.grids.iter().flat_map(|grid| grid.compositions().iter().copied())
            .map(TernaryPoint::from).collect::<Vec<_>>();
        chart.draw_series(TernaryPointSeries::new(points).marker(MarkerShape::Circle).size(if options.show_grid { 3 } else { 5 }).style(BLACK.mix(0.35).filled()))?;
    }
    chart.configure_series_labels().background_style(WHITE.mix(0.82)).border_style(BLACK).draw()?;
    Ok(())
}

fn phase_style(phase: StablePhaseId) -> ShapeStyle {
    const PALETTE: [RGBColor; 8] = [
        RGBColor(31, 119, 180), RGBColor(214, 39, 40), RGBColor(44, 160, 44),
        RGBColor(148, 103, 189), RGBColor(255, 127, 14), RGBColor(23, 190, 207),
        RGBColor(140, 86, 75), RGBColor(227, 119, 194),
    ];
    PALETTE[phase.0 as usize % PALETTE.len()].stroke_width(2)
}
