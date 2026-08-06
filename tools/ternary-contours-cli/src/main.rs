use std::{error::Error, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use ternary_contours::{
    BinaryExtrapolation, CubicAlphaMethod, CubicPartialDomainPolicy,
    RegularMeshExtrapolationOptions, StableInvariantNode,
};
#[cfg(feature = "trace")]
use ternary_contours::{
    NumericalTraceConfig, NumericalTraceEventKind, NumericalTraceLevel, TraceBinaryBoundary,
};
use ternary_contours_cli::{
    IrregularTemplateStyle, MeshExtrapolationField, MeshExtrapolationRequest, NumericFormat,
    OutputFormat, ProjectionOptions, RenderOptions, TabulatedGrid, TctSerializeOptions,
    apply_mesh_extrapolation, audit_cao_pbo_zno_binary_edges, calculate_projection,
    compositions_tsv, extrapolate_regular_grid_fields, irregular_template, parse_components,
    parse_field_specs, parse_level_spec, parse_path, regular_template_tct, render_to_path,
    save_tct_atomic, serialize_tct,
};

#[derive(Parser)]
#[command(
    name = "ternary-contours-cli",
    version,
    about = "Inspect, validate, plot, and view TCT liquidus tables"
)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Inspect {
        input: PathBuf,
    },
    Validate {
        input: PathBuf,
        #[arg(long)]
        warnings_as_errors: bool,
    },
    Plot {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[command(flatten)]
        options: PlotOptions,
    },
    /// Emit canonical regular-grid composition TSV for Excel or other tools.
    Compositions(CompositionsArgs),
    /// Generate a regular or irregular TCT data-entry template.
    Template {
        #[command(subcommand)]
        kind: TemplateCommand,
    },
    /// Audit exact finite overlap and linear roots on CaO–PbO–ZnO source edges.
    /// Fill eligible NA cells on a regular mesh with provenance-preserving EX values.
    ExtrapolateMesh(ExtrapolateMeshArgs),
    AuditBinaryEdges(AuditBinaryEdgesArgs),
    /// Run a deterministic stable-topology repeatability and resolution audit.
    AuditStableTopology(AuditStableTopologyArgs),
    /// Produce an opt-in numerical projection trace (requires `--features trace`).
    TraceProjection(TraceProjectionArgs),
    /// Analyze a numerical trace JSON Lines file (requires `--features trace`).
    AnalyzeTrace {
        input: PathBuf,
    },
    /// Open the optional native liquidus inspection viewer.
    View {
        input: Option<PathBuf>,
        #[command(flatten)]
        options: PlotOptions,
    },
}

#[derive(Args)]
struct CompositionsArgs {
    #[arg(long)]
    subdivisions: usize,
    #[arg(long)]
    components: String,
    #[arg(long)]
    header: bool,
    #[arg(long, default_value_t = 6)]
    precision: usize,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum TemplateCommand {
    Regular(RegularTemplateArgs),
    Irregular(IrregularTemplateArgs),
}

#[derive(Args)]
struct RegularTemplateArgs {
    #[arg(long)]
    subdivisions: usize,
    #[arg(long)]
    components: String,
    #[arg(long)]
    fields: String,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long, default_value_t = 6)]
    precision: usize,
}

#[derive(Args)]
struct IrregularTemplateArgs {
    #[arg(long)]
    components: String,
    #[arg(long)]
    fields: String,
    #[arg(long, value_enum, default_value_t = TemplateStyleArg::FullTct)]
    style: TemplateStyleArg,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum TemplateStyleArg {
    FullTct,
    GridSection,
    TsvHeader,
}

impl From<TemplateStyleArg> for IrregularTemplateStyle {
    fn from(value: TemplateStyleArg) -> Self {
        match value {
            TemplateStyleArg::FullTct => Self::FullTct,
            TemplateStyleArg::GridSection => Self::GridSection,
            TemplateStyleArg::TsvHeader => Self::TsvHeader,
        }
    }
}
/// Options shared by static plotting and the interactive viewer.
#[derive(Args, Clone, Default)]
struct PlotOptions {
    #[arg(long)]
    levels: Option<String>,
    #[arg(long)]
    sampling_subdivisions: Option<usize>,
    #[arg(long, conflicts_with = "no_regularize")]
    regularize: bool,
    #[arg(long)]
    no_regularize: bool,
    #[arg(long)]
    show_isotherms: bool,
    #[arg(long)]
    show_univariants: bool,
    #[arg(long)]
    show_invariants: bool,
    #[arg(long)]
    show_binary_invariants: bool,
    #[arg(long)]
    show_grid: bool,
    #[arg(long)]
    show_samples: bool,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long, value_enum)]
    format: Option<FormatArg>,
}

impl PlotOptions {
    fn projection_options(&self) -> Result<ProjectionOptions, Box<dyn Error>> {
        let levels = self.levels.as_deref().map(parse_level_spec).transpose()?;
        Ok(ProjectionOptions {
            levels: levels.unwrap_or_default(),
            sampling_subdivisions: self.sampling_subdivisions,
            regularize: self.regularize && !self.no_regularize,
            ..ProjectionOptions::default()
        })
    }

    fn render_options(&self) -> RenderOptions {
        let selected_layers = self.show_isotherms
            || self.show_univariants
            || self.show_invariants
            || self.show_binary_invariants;
        let mut render = RenderOptions::default();
        if selected_layers {
            render.show_isotherms = self.show_isotherms;
            render.show_univariants = self.show_univariants;
            render.show_invariants = self.show_invariants;
            render.show_binary_invariants = self.show_binary_invariants;
        }
        render.show_grid = self.show_grid;
        render.show_samples = self.show_samples;
        render.width = self.width.unwrap_or(render.width);
        render.height = self.height.unwrap_or(render.height);
        render.title = self.title.clone();
        render
    }
}

#[derive(Args)]
struct ExtrapolateMeshArgs {
    input: PathBuf,
    #[arg(long)]
    grid: String,
    /// Phase/property field, e.g. Lime.T. May be repeated.
    #[arg(long = "field")]
    fields: Vec<String>,
    #[arg(long, conflicts_with = "fields")]
    all_fields: bool,
    #[arg(long, value_enum, default_value_t = CubicMethodArg::Steffen)]
    method: CubicMethodArg,
    #[arg(long, default_value_t = 1)]
    max_layers: u16,
    #[arg(long, default_value_t = 3)]
    minimum_support: usize,
    #[arg(long)]
    max_spread: Option<f64>,
    #[arg(long)]
    minimum_value: Option<f64>,
    #[arg(long)]
    maximum_value: Option<f64>,
    /// Review the proposed EX cells without writing an output TCT.
    #[arg(long)]
    preview: bool,
    #[arg(short, long)]
    output: Option<PathBuf>,
}
#[derive(Args)]
struct AuditBinaryEdgesArgs {
    input: PathBuf,
    /// CSV destination; the Markdown report is written beside it with `.md`.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, value_enum, default_value_t = SourceInterpolationArg::Linear)]
    source_interpolation: SourceInterpolationArg,
}
#[derive(Args)]
struct AuditStableTopologyArgs {
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, value_delimiter = ',', default_value = "6,7,8,10,12,16,20,30,40")]
    sampling_subdivisions: Vec<usize>,
    #[arg(long, default_value_t = 5)]
    repeat_count: usize,
    #[arg(long)]
    regularize: bool,
    #[arg(long)]
    regularization_spacing: Option<f64>,
}

#[derive(Args)]
struct TraceProjectionArgs {
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long, value_enum, default_value_t = TraceLevelArg::Decisions)]
    level: TraceLevelArg,
    #[arg(long, default_value_t = 500_000)]
    max_events: usize,
    #[arg(long, value_enum)]
    boundary: Option<TraceBoundaryArg>,
    #[arg(long)]
    phase: Option<u32>,
    #[arg(long)]
    phase_pair: Option<String>,
    #[arg(long)]
    triangle: Option<usize>,
    #[arg(long)]
    event: Option<String>,
    #[arg(long)]
    levels: Option<String>,
    #[arg(long)]
    sampling_subdivisions: Option<usize>,
    #[arg(long)]
    regularize: bool,
    #[arg(long)]
    regularization_spacing: Option<f64>,
    #[arg(long, value_enum, default_value_t = SourceInterpolationArg::Linear)]
    source_interpolation: SourceInterpolationArg,
    #[arg(long, value_enum, default_value_t = CubicMethodArg::Steffen)]
    cubic_method: CubicMethodArg,
    #[arg(long, value_enum, default_value_t = PartialDomainArg::OneSidedThenLinear)]
    partial_domain_policy: PartialDomainArg,
    #[arg(long, value_enum, default_value_t = ContinuationArg::Muggianu)]
    continuation: ContinuationArg,
    #[arg(long)]
    automatic_range: bool,
    #[arg(long)]
    tmin: Option<f64>,
    #[arg(long)]
    tmax: Option<f64>,
    #[arg(long)]
    step: Option<f64>,
}

#[derive(Clone, Copy, ValueEnum)]
enum TraceLevelArg {
    Off,
    Summary,
    Decisions,
    Iterations,
}

#[derive(Clone, Copy, ValueEnum)]
enum TraceBoundaryArg {
    Ab,
    Bc,
    Ca,
}

#[derive(Clone, Copy, ValueEnum)]
enum SourceInterpolationArg {
    Linear,
    CubicAlpha,
}

#[derive(Clone, Copy, ValueEnum)]
enum CubicMethodArg {
    Akima,
    Makima,
    Pchip,
    Steffen,
}

#[derive(Clone, Copy, ValueEnum)]
enum PartialDomainArg {
    StrictCubic,
    OneSidedCubic,
    OneSidedThenLinear,
    LinearNearBoundaries,
}

#[derive(Clone, Copy, ValueEnum)]
enum ContinuationArg {
    RawBarycentric,
    Muggianu,
    Kohler,
}
#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Svg,
    Png,
}

impl From<FormatArg> for OutputFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Svg => Self::Svg,
            FormatArg::Png => Self::Png,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose;
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if verbose {
                let mut cause = error.source();
                while let Some(error) = cause {
                    eprintln!("caused by: {error}");
                    cause = error.source();
                }
            }
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Some(Command::Inspect { input }) => inspect(input),
        Some(Command::Validate {
            input,
            warnings_as_errors,
        }) => validate(input, warnings_as_errors),
        Some(Command::Plot {
            input,
            output,
            options,
        }) => plot(input, output, options),
        Some(Command::Compositions(args)) => compositions(args),
        Some(Command::Template { kind }) => template(kind),
        Some(Command::ExtrapolateMesh(args)) => extrapolate_mesh(args),
        Some(Command::AuditBinaryEdges(args)) => audit_binary_edges(args),
        Some(Command::AuditStableTopology(args)) => audit_stable_topology(args),
        Some(Command::TraceProjection(args)) => trace_projection(args),
        Some(Command::AnalyzeTrace { input }) => analyze_trace_command(input),
        Some(Command::View { input, options }) => view(input, options),
        None => view(None, PlotOptions::default()),
    }
}

fn inspect(input: PathBuf) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    println!("Format: TCT {}", dataset.version);
    println!(
        "Components: {}, {}, {}",
        dataset.components[0].name, dataset.components[1].name, dataset.components[2].name
    );
    println!("Phases: {}", dataset.phases.len());
    println!(
        "Properties: {}",
        dataset
            .properties
            .iter()
            .map(|property| property.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("Grids: {}", dataset.grids.len());
    for grid in &dataset.grids {
        let (defined, undefined) =
            grid.fields()
                .iter()
                .fold((0, 0), |(defined, undefined), field| {
                    let values = field.values.iter();
                    (
                        defined + values.clone().filter(|value| value.is_defined()).count(),
                        undefined + values.filter(|value| !value.is_calculated()).count(),
                    )
                });
        match grid {
            TabulatedGrid::Regular(grid) => println!(
                "- {}: regular, n={}, fields={}, defined={}, undefined={}, completeness=complete",
                grid.name,
                grid.subdivisions,
                grid.fields.len(),
                defined,
                undefined
            ),
            TabulatedGrid::Irregular(grid) => println!(
                "- {}: irregular, points={}, fields={}, defined={}, undefined={}, hull=non-collinear validated",
                grid.name,
                grid.compositions.len(),
                grid.fields.len(),
                defined,
                undefined
            ),
        }
    }
    for warning in &dataset.warnings {
        println!("Warning: {warning}");
    }
    Ok(())
}

fn validate(input: PathBuf, warnings_as_errors: bool) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    if warnings_as_errors && !dataset.warnings.is_empty() {
        return Err(format!(
            "validation warnings are errors: {}",
            dataset.warnings.join("; ")
        )
        .into());
    }
    let projection = calculate_projection(&dataset, &ProjectionOptions::default())?;
    println!(
        "Valid: {} grids, {} phases, {} isotherm paths, {} invariants, {} univariants",
        dataset.grids.len(),
        dataset.phases.len(),
        projection.diagnostics.contour_path_count,
        projection.diagnostics.invariant_count,
        projection.diagnostics.univariant_count,
    );
    Ok(())
}

fn plot(input: PathBuf, output: PathBuf, options: PlotOptions) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    let projection = calculate_projection(&dataset, &options.projection_options()?)?;
    let render = options.render_options();
    render_to_path(
        &output,
        &dataset,
        &projection,
        &render,
        options.format.map(Into::into),
    )?;
    println!(
        "Wrote {} ({} isotherm paths, {} invariants, {} univariants)",
        output.display(),
        projection.diagnostics.contour_path_count,
        projection.diagnostics.invariant_count,
        projection.diagnostics.univariant_count,
    );
    Ok(())
}

fn compositions(args: CompositionsArgs) -> Result<(), Box<dyn Error>> {
    let components = parse_components(&args.components)?;
    let content = compositions_tsv(
        args.subdivisions,
        [&components[0], &components[1], &components[2]],
        args.header,
        NumericFormat {
            decimal_places: args.precision,
        },
    )?;
    write_generated(args.output, content)
}

fn template(kind: TemplateCommand) -> Result<(), Box<dyn Error>> {
    match kind {
        TemplateCommand::Regular(args) => {
            let content = regular_template_tct(
                args.subdivisions,
                parse_components(&args.components)?,
                &parse_field_specs(&args.fields)?,
                NumericFormat {
                    decimal_places: args.precision,
                },
            )?;
            write_generated(args.output, content)
        }
        TemplateCommand::Irregular(args) => {
            let content = irregular_template(
                parse_components(&args.components)?,
                &parse_field_specs(&args.fields)?,
                args.style.into(),
            );
            write_generated(args.output, content)
        }
    }
}

fn extrapolate_mesh(args: ExtrapolateMeshArgs) -> Result<(), Box<dyn Error>> {
    let mut dataset = parse_path(&args.input)?;
    let request = MeshExtrapolationRequest {
        grid: args.grid.clone(),
        fields: args
            .fields
            .iter()
            .map(|field| MeshExtrapolationField::parse(field))
            .collect::<Result<_, _>>()?,
        all_fields: args.all_fields,

        target_rows: Vec::new(),
        options: RegularMeshExtrapolationOptions {
            method: args.method.into(),
            maximum_layers: args.max_layers,
            minimum_directional_support: args.minimum_support,
            maximum_directional_spread: args.max_spread,
            minimum_value: args.minimum_value,
            maximum_value: args.maximum_value,
            ..RegularMeshExtrapolationOptions::default()
        },
    };
    let preview = extrapolate_regular_grid_fields(&dataset, &request)?;
    println!("Grid: {}", preview.grid_name);
    println!("Fields processed: {}", preview.fields.len());
    println!("Method: {:?}", preview.options.method);
    for field in &preview.fields {
        println!(
            "{}.{}: EX values created: {}; layers completed: {}; remaining NA: {}; rejected for insufficient support: {}; rejected for directional disagreement: {}",
            field.phase_id.0,
            field.property,
            field.diagnostics.values_created,
            field.diagnostics.layers_completed,
            field.diagnostics.remaining_eligible_missing_values,
            field.diagnostics.rejected_insufficient_support,
            field.diagnostics.rejected_directional_spread,
        );
    }
    if args.preview {
        return Ok(());
    }
    let output = args
        .output
        .ok_or("--output is required unless --preview is specified")?;
    let summary = apply_mesh_extrapolation(&mut dataset, preview)?;
    let contents = serialize_tct(&dataset, &TctSerializeOptions::default())?;
    save_tct_atomic(&output, &contents)?;
    println!(
        "Wrote {} ({} EX values across {} fields; maximum layer EX{})",
        output.display(),
        summary.values_created,
        summary.fields_changed,
        summary.maximum_layer
    );
    Ok(())
}
fn audit_binary_edges(args: AuditBinaryEdgesArgs) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&args.input)?;
    let source_interpolation = match args.source_interpolation {
        SourceInterpolationArg::Linear => ternary_contours_cli::SourceInterpolation::Linear,
        SourceInterpolationArg::CubicAlpha => {
            return Err(
                "audit-binary-edges currently supports exact Linear source interpolation only"
                    .into(),
            );
        }
    };
    let report = audit_cao_pbo_zno_binary_edges(
        &dataset,
        &ProjectionOptions {
            interpolation: ternary_contours_cli::InterpolationOptions {
                source: source_interpolation,
                ..ternary_contours_cli::InterpolationOptions::default()
            },
            ..ProjectionOptions::default()
        },
    )?;
    let scanner_options = ProjectionOptions {
        automatic_level_step: Some(100.0),
        sampling_subdivisions: Some(20),
        regularize: true,
        interpolation: ternary_contours_cli::InterpolationOptions {
            source: source_interpolation,
            ..ternary_contours_cli::InterpolationOptions::default()
        },
        ..ProjectionOptions::default()
    };
    let scanner_projection = calculate_projection(&dataset, &scanner_options)?;
    let scanner_binary = scanner_projection
        .stable_boundaries
        .nodes
        .iter()
        .filter(|node| matches!(node, StableInvariantNode::Binary(_)))
        .count();
    let scanner_unavailable = scanner_projection
        .stable_boundaries
        .binary_traces
        .iter()
        .map(|trace| trace.incomplete_transitions.len())
        .sum::<usize>();
    let markdown = args.output.with_extension("md");
    let scanner_summary = format!(
        "\n## Current binary scanner comparison\n\nQt-equivalent Linear options (sampling 20, regularization enabled) produced {scanner_binary} binary invariant(s) and {scanner_unavailable} typed unavailable transition(s). The independent raw-edge audit agrees: AB and BC contain no finite sign-changing overlap; CA contains the sole stable root.\n"
    );
    std::fs::write(&args.output, report.to_csv())?;
    std::fs::write(&markdown, report.to_markdown() + &scanner_summary)?;
    println!("Wrote {} and {}", args.output.display(), markdown.display());
    for edge in &report.edges {
        let roots = edge
            .intervals
            .iter()
            .filter(|interval| interval.root.is_some())
            .count();
        let changes = edge
            .intervals
            .iter()
            .filter(|interval| interval.sign_change)
            .count();
        println!(
            "{:?}: {} finite intervals, {} sign-changing intervals, {} roots",
            edge.edge,
            edge.intervals.len(),
            changes,
            roots
        );
    }
    Ok(())
}
fn write_generated(output: Option<PathBuf>, content: String) -> Result<(), Box<dyn Error>> {
    if let Some(output) = output {
        std::fs::write(&output, content)?;
        println!("Wrote {}", output.display());
    } else {
        print!("{content}");
    }
    Ok(())
}
fn audit_stable_topology(args: AuditStableTopologyArgs) -> Result<(), Box<dyn Error>> {
    if args.repeat_count == 0 || args.sampling_subdivisions.is_empty() {
        return Err("repeat count and sampling subdivisions must be positive".into());
    }
    std::fs::create_dir_all(&args.output)?;
    let input_bytes = std::fs::read(&args.input)?;
    let input_hash = input_bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let dataset = parse_path(&args.input)?;
    let mut runs = String::from(
        "sampling\trepeat\tstatus\tsignature\tbinary\tinterior\tunivariants\ttruncated\tregularization_failures\n",
    );
    let mut invariants =
        String::from("sampling\trepeat\tkind\tphases\ta\tb\tc\ttemperature\tdegree\n");
    let mut univariants = String::from("sampling\trepeat\tphases\tstart\tend\traw_points\tstate\n");
    let mut failures = String::from("sampling\trepeat\tpath\tphases\terror\n");
    let mut signatures = Vec::new();
    for &sampling in &args.sampling_subdivisions {
        for repeat in 0..args.repeat_count {
            let options = ProjectionOptions {
                automatic_level_step: Some(100.0),
                sampling_subdivisions: Some(sampling),
                regularize: args.regularize,
                regularization_spacing: args.regularization_spacing,
                ..ProjectionOptions::default()
            };
            match calculate_projection(&dataset, &options) {
                Ok(projection) => {
                    let mut node_records = projection
                        .stable_boundaries
                        .nodes
                        .iter()
                        .map(|node| {
                            let kind = if matches!(node, StableInvariantNode::Binary(_)) {
                                "binary"
                            } else {
                                "interior"
                            };
                            let mut phases = node
                                .phases()
                                .iter()
                                .map(|phase| phase.0)
                                .collect::<Vec<_>>();
                            phases.sort();
                            let point = node.point().as_array();
                            format!(
                                "{kind}:{phases:?}:{:.10}:{:.10}:{:.10}:{:.8}",
                                point[0],
                                point[1],
                                point[2],
                                node.temperature()
                            )
                        })
                        .collect::<Vec<_>>();
                    node_records.sort();
                    let mut edge_records = projection
                        .stable_boundaries
                        .univariants
                        .iter()
                        .map(|path| {
                            let mut phases = [path.phases.first.0, path.phases.second.0];
                            phases.sort();
                            format!(
                                "{:?}:{}:{}",
                                phases,
                                path.start.0.min(path.end.0),
                                path.start.0.max(path.end.0)
                            )
                        })
                        .collect::<Vec<_>>();
                    edge_records.sort();
                    let signature =
                        format!("{}|{}", node_records.join(";"), edge_records.join(";"));
                    signatures.push((sampling, repeat, signature.clone()));
                    let binary = projection
                        .stable_boundaries
                        .nodes
                        .iter()
                        .filter(|node| matches!(node, StableInvariantNode::Binary(_)))
                        .count();
                    let interior = projection.stable_boundaries.nodes.len() - binary;
                    runs.push_str(&format!("{sampling}\t{repeat}\tok\t{signature:?}\t{binary}\t{interior}\t{}\t{}\t{}\n", projection.stable_boundaries.univariants.len(), projection.stable_boundaries.truncated_univariants.len(), projection.stable_boundaries.regularization_failures.len()));
                    for node in &projection.stable_boundaries.nodes {
                        let kind = if matches!(node, StableInvariantNode::Binary(_)) {
                            "binary"
                        } else {
                            "interior"
                        };
                        let point = node.point().as_array();
                        let phases = node
                            .phases()
                            .iter()
                            .map(|phase| phase.0.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        let degree = projection
                            .stable_boundaries
                            .incident_univariants(node.id())?
                            .len();
                        invariants.push_str(&format!("{sampling}\t{repeat}\t{kind}\t{phases}\t{:.16}\t{:.16}\t{:.16}\t{:.16}\t{degree}\n",point[0],point[1],point[2],node.temperature()));
                    }
                    for path in &projection.stable_boundaries.univariants {
                        let state = match projection
                            .stable_boundaries
                            .path_geometry_state(path.id)
                            .expect("path belongs to network")
                        {
                            ternary_contours::StablePathGeometryState::Raw => "raw",
                            ternary_contours::StablePathGeometryState::Regularized => "regularized",
                            ternary_contours::StablePathGeometryState::RawFallback => {
                                "raw_fallback"
                            }
                        };
                        univariants.push_str(&format!(
                            "{sampling}\t{repeat}\t{},{}\t{}\t{}\t{}\t{state}\n",
                            path.phases.first.0,
                            path.phases.second.0,
                            path.start.0,
                            path.end.0,
                            path.points.len()
                        ));
                    }
                    for failure in &projection.stable_boundaries.regularization_failures {
                        failures.push_str(&format!(
                            "{sampling}\t{repeat}\t{}\t{},{}\t{}\n",
                            failure.path.0,
                            failure.phases.first.0,
                            failure.phases.second.0,
                            failure.error
                        ));
                    }
                }
                Err(error) => runs.push_str(&format!(
                    "{sampling}\t{repeat}\terror\t{:?}\t0\t0\t0\t0\t0\n",
                    error.to_string()
                )),
            }
        }
    }
    let repeatable = signatures
        .iter()
        .filter(|(sampling, _, _)| *sampling == 20)
        .map(|(_, _, signature)| signature)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        <= 1;
    let summary = format!(
        "{{\n  \"input\": {:?},\n  \"fnv1a64\": \"{:016x}\",\n  \"repeatable_at_20\": {},\n  \"runs\": {}\n}}\n",
        args.input,
        input_hash,
        repeatable,
        signatures.len()
    );
    let report = format!(
        "# Stable topology audit\n\nInput: `{}`\n\nRepeatable at sampling 20: **{}**.\n\nCanonical signatures are stored in `runs.tsv`; node and edge details are stored separately.\n",
        args.input.display(),
        repeatable
    );
    std::fs::write(args.output.join("summary.json"), summary)?;
    std::fs::write(args.output.join("runs.tsv"), runs)?;
    std::fs::write(args.output.join("invariants.tsv"), invariants)?;
    std::fs::write(args.output.join("univariants.tsv"), univariants)?;
    std::fs::write(args.output.join("regularization-failures.tsv"), failures)?;
    std::fs::write(args.output.join("comparison-report.md"), report)?;
    println!("Stable topology audit written to {}", args.output.display());
    Ok(())
}

#[cfg(feature = "trace")]
fn trace_projection(args: TraceProjectionArgs) -> Result<(), Box<dyn Error>> {
    let input_content_hash = trace_input_hash(&args.input)?;
    let dataset = parse_path(&args.input)?;
    let options = trace_projection_options(&args)?;
    let mut sink = ternary_contours_cli::JsonLinesTraceSink::create(
        &args.output,
        NumericalTraceConfig {
            level: args.level.into(),
            maximum_events: args.max_events,
            event_filter: args.event.as_deref().map(parse_trace_event).transpose()?,
            boundary_filter: args.boundary.map(Into::into),
            phase_filter: args.phase,
            phase_pair_filter: args
                .phase_pair
                .as_deref()
                .map(parse_phase_pair)
                .transpose()?,
            triangle_filter: args.triangle,
            composition_region: None,
        },
    )?;
    let context = ternary_contours_cli::NumericalTraceRunContext {
        input_identifier: Some(args.input.display().to_string()),
        input_content_hash: Some(input_content_hash),
        ..ternary_contours_cli::NumericalTraceRunContext::default()
    };
    let projection = ternary_contours_cli::calculate_projection_with_trace_context(
        &dataset, &options, &mut sink, &context,
    );
    let status = sink.finish();
    match projection {
        Ok(projection) => {
            println!(
                "Trace written to {}\nEvents: {}\nTruncated: {}\nProjection: {} invariants, {} univariants, {} isotherm paths",
                status.path.display(),
                status.events_written,
                if status.truncated { "yes" } else { "no" },
                projection.diagnostics.invariant_count,
                projection.diagnostics.univariant_count,
                projection.diagnostics.contour_path_count,
            );
            if let Some(error) = status.first_error {
                return Err(format!(
                    "projection succeeded but trace output failed for {}: {error}",
                    status.path.display()
                )
                .into());
            }
            Ok(())
        }
        Err(error) => {
            if let Some(output_error) = status.first_error {
                eprintln!(
                    "trace output also failed for {}: {output_error}",
                    status.path.display()
                );
            }
            Err(error.into())
        }
    }
}

#[cfg(not(feature = "trace"))]
fn trace_projection(_args: TraceProjectionArgs) -> Result<(), Box<dyn Error>> {
    Err("trace-projection requires `--features trace`".into())
}

#[cfg(feature = "trace")]
fn analyze_trace_command(input: PathBuf) -> Result<(), Box<dyn Error>> {
    let analysis = ternary_contours_cli::analyze_trace(&input)
        .map_err(|error| format!("could not analyze {}: {error}", input.display()))?;
    println!(
        "Schema: {:?}\nEvents: {}",
        analysis.schema_versions, analysis.event_count
    );
    println!(
        "Binary transitions/scans: {}",
        analysis.binary_boundaries_started
    );
    println!(
        "Confirmed invariants: {} binary, {} ternary",
        analysis.binary_invariants, analysis.interior_invariants
    );
    println!(
        "Unavailable binary transitions: {}",
        analysis.unavailable_binary_transitions
    );
    println!("Completed univariants: {}", analysis.univariants_completed);
    println!("Completed contour paths: {}", analysis.contours_completed);
    for warning in &analysis.warnings {
        println!("warning: {warning}");
    }
    if analysis.is_consistent() {
        Ok(())
    } else {
        Err("trace has structural warnings or is incomplete".into())
    }
}

#[cfg(not(feature = "trace"))]
fn analyze_trace_command(_input: PathBuf) -> Result<(), Box<dyn Error>> {
    Err("analyze-trace requires `--features trace`".into())
}

#[cfg(feature = "trace")]
fn trace_input_hash(path: &std::path::Path) -> Result<String, Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    Ok(format!("fnv1a64:{hash:016x}"))
}

#[cfg(feature = "trace")]
fn trace_projection_options(
    args: &TraceProjectionArgs,
) -> Result<ProjectionOptions, Box<dyn Error>> {
    if args.levels.is_some() && (args.tmin.is_some() || args.tmax.is_some()) {
        return Err("--levels cannot be combined with --tmin/--tmax".into());
    }
    let mut options = ProjectionOptions {
        levels: args
            .levels
            .as_deref()
            .map(parse_level_spec)
            .transpose()?
            .unwrap_or_default(),
        sampling_subdivisions: args.sampling_subdivisions,
        regularize: args.regularize,
        regularization_spacing: args.regularization_spacing,
        interpolation: ternary_contours_cli::InterpolationOptions {
            source: match args.source_interpolation {
                SourceInterpolationArg::Linear => ternary_contours_cli::SourceInterpolation::Linear,
                SourceInterpolationArg::CubicAlpha => {
                    ternary_contours_cli::SourceInterpolation::CubicAlpha {
                        method: args.cubic_method.into(),
                        continuation: args.continuation.into(),
                    }
                }
            },
            partial_domain_policy: args.partial_domain_policy.into(),
        },
        ..ProjectionOptions::default()
    };
    if let (Some(minimum), Some(maximum)) = (args.tmin, args.tmax) {
        let step = args.step.unwrap_or(100.0);
        options.levels = ternary_contours_cli::automatic_iso_levels(minimum, maximum, step)?;
    } else if args.tmin.is_some() || args.tmax.is_some() {
        return Err("--tmin and --tmax must be supplied together".into());
    } else if args.automatic_range {
        options.automatic_level_step = Some(args.step.unwrap_or(100.0));
    } else if args.step.is_some() {
        return Err("--step requires --automatic-range or --tmin with --tmax".into());
    }
    Ok(options)
}

#[cfg(feature = "trace")]
fn parse_phase_pair(input: &str) -> Result<[u32; 2], Box<dyn Error>> {
    let mut values = input.split(',').map(str::trim).map(str::parse::<u32>);
    let first = values.next().ok_or("--phase-pair must be ID,ID")??;
    let second = values.next().ok_or("--phase-pair must be ID,ID")??;
    if values.next().is_some() || first == second {
        return Err("--phase-pair must contain two distinct integer IDs".into());
    }
    Ok([first.min(second), first.max(second)])
}

#[cfg(feature = "trace")]
fn parse_trace_event(input: &str) -> Result<NumericalTraceEventKind, Box<dyn Error>> {
    serde_json::from_str(&format!("\"{}\"", input.trim().to_ascii_lowercase())).map_err(|_| {
        format!("unsupported trace event `{input}`; use a documented snake_case event name").into()
    })
}

#[cfg(feature = "trace")]
impl From<TraceLevelArg> for NumericalTraceLevel {
    fn from(value: TraceLevelArg) -> Self {
        match value {
            TraceLevelArg::Off => Self::Off,
            TraceLevelArg::Summary => Self::Summary,
            TraceLevelArg::Decisions => Self::Decisions,
            TraceLevelArg::Iterations => Self::Iterations,
        }
    }
}

#[cfg(feature = "trace")]
impl From<TraceBoundaryArg> for TraceBinaryBoundary {
    fn from(value: TraceBoundaryArg) -> Self {
        match value {
            TraceBoundaryArg::Ab => Self::Ab,
            TraceBoundaryArg::Bc => Self::Bc,
            TraceBoundaryArg::Ca => Self::Ca,
        }
    }
}

impl From<CubicMethodArg> for CubicAlphaMethod {
    fn from(value: CubicMethodArg) -> Self {
        match value {
            CubicMethodArg::Akima => Self::Akima,
            CubicMethodArg::Makima => Self::Makima,
            CubicMethodArg::Pchip => Self::Pchip,
            CubicMethodArg::Steffen => Self::Steffen,
        }
    }
}

impl From<PartialDomainArg> for CubicPartialDomainPolicy {
    fn from(value: PartialDomainArg) -> Self {
        match value {
            PartialDomainArg::StrictCubic => Self::Strict,
            PartialDomainArg::OneSidedCubic => Self::OneSided,
            PartialDomainArg::OneSidedThenLinear => Self::OneSidedThenLinear,
            PartialDomainArg::LinearNearBoundaries => Self::LinearNearDomain,
        }
    }
}

impl From<ContinuationArg> for BinaryExtrapolation {
    fn from(value: ContinuationArg) -> Self {
        match value {
            ContinuationArg::RawBarycentric => Self::RawBarycentric,
            ContinuationArg::Muggianu => Self::Muggianu,
            ContinuationArg::Kohler => Self::Kohler,
        }
    }
}
#[cfg(feature = "viewer")]
fn view(input: Option<PathBuf>, options: PlotOptions) -> Result<(), Box<dyn Error>> {
    ternary_contours_cli::viewer::launch(
        input,
        options.projection_options()?,
        options.render_options(),
    )
}

#[cfg(not(feature = "viewer"))]
fn view(_input: Option<PathBuf>, _options: PlotOptions) -> Result<(), Box<dyn Error>> {
    Err("the `view` command requires the optional `viewer` feature; rerun with `cargo run -p ternary-contours-cli --features viewer -- view <input.tct>`".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_argument_routes_to_default_view_request() {
        let cli = Cli::try_parse_from(["ternary-contours-cli"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn view_input_is_optional_but_file_backed_view_remains_supported() {
        let no_file = Cli::try_parse_from(["ternary-contours-cli", "view"]).unwrap();
        assert!(matches!(
            no_file.command,
            Some(Command::View { input: None, .. })
        ));
        let with_file =
            Cli::try_parse_from(["ternary-contours-cli", "view", "existing.tct"]).unwrap();
        assert!(matches!(
            with_file.command,
            Some(Command::View { input: Some(path), .. }) if path == std::path::Path::new("existing.tct")
        ));
    }

    #[test]
    fn headless_subcommands_remain_compatible() {
        for args in [
            vec!["ternary-contours-cli", "inspect", "file.tct"],
            vec!["ternary-contours-cli", "validate", "file.tct"],
            vec![
                "ternary-contours-cli",
                "plot",
                "file.tct",
                "--output",
                "plot.svg",
            ],
            vec![
                "ternary-contours-cli",
                "compositions",
                "--subdivisions",
                "2",
                "--components",
                "A,B,C",
            ],
            vec![
                "ternary-contours-cli",
                "template",
                "regular",
                "--subdivisions",
                "2",
                "--components",
                "A,B,C",
                "--fields",
                "Phase1.T",
            ],
        ] {
            assert!(
                Cli::try_parse_from(args.clone()).is_ok(),
                "failed to parse {args:?}"
            );
        }
    }
}
