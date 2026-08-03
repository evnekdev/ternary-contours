use std::{error::Error, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use ternary_contours_cli::{
    OutputFormat, ProjectionOptions, RenderOptions, TabulatedGrid, calculate_projection,
    parse_level_spec, parse_path, render_to_path,
};

#[derive(Parser)]
#[command(name = "ternary-contours-cli", version, about = "Inspect, validate, and plot TCT liquidus tables")]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Inspect { input: PathBuf },
    Validate { input: PathBuf, #[arg(long)] warnings_as_errors: bool },
    Plot { input: PathBuf, #[arg(short, long)] output: PathBuf, #[command(flatten)] options: PlotOptions },
}

#[derive(Args)]
struct PlotOptions {
    #[arg(long)] levels: Option<String>,
    #[arg(long)] sampling_subdivisions: Option<usize>,
    #[arg(long, conflicts_with = "no_regularize")] regularize: bool,
    #[arg(long)] no_regularize: bool,
    #[arg(long)] show_isotherms: bool,
    #[arg(long)] show_univariants: bool,
    #[arg(long)] show_invariants: bool,
    #[arg(long)] show_binary_invariants: bool,
    #[arg(long)] show_grid: bool,
    #[arg(long)] show_samples: bool,
    #[arg(long)] width: Option<u32>,
    #[arg(long)] height: Option<u32>,
    #[arg(long)] title: Option<String>,
    #[arg(long, value_enum)] format: Option<FormatArg>,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg { Svg, Png }

impl From<FormatArg> for OutputFormat {
    fn from(value: FormatArg) -> Self {
        match value { FormatArg::Svg => Self::Svg, FormatArg::Png => Self::Png }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Inspect { input } => inspect(input),
        Command::Validate { input, warnings_as_errors } => validate(input, warnings_as_errors),
        Command::Plot { input, output, options } => plot(input, output, options),
    }
}

fn inspect(input: PathBuf) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    println!("Format: TCT {}", dataset.version);
    println!("Components: {}, {}, {}", dataset.components[0].name, dataset.components[1].name, dataset.components[2].name);
    println!("Phases: {}", dataset.phases.len());
    println!("Properties: {}", dataset.properties.iter().map(|property| property.name.as_str()).collect::<Vec<_>>().join(", "));
    println!("Grids: {}", dataset.grids.len());
    for grid in &dataset.grids {
        let (defined, undefined) = grid.fields().iter().fold((0, 0), |(defined, undefined), field| {
            let values = field.values.iter();
            (defined + values.clone().flatten().count(), undefined + values.filter(|value| value.is_none()).count())
        });
        match grid {
            TabulatedGrid::Regular(grid) => println!(
                "- {}: regular, n={}, fields={}, defined={}, undefined={}, completeness=complete",
                grid.name, grid.subdivisions, grid.fields.len(), defined, undefined
            ),
            TabulatedGrid::Irregular(grid) => println!(
                "- {}: irregular, points={}, fields={}, defined={}, undefined={}, hull=non-collinear validated",
                grid.name, grid.compositions.len(), grid.fields.len(), defined, undefined
            ),
        }
    }
    for warning in &dataset.warnings { println!("Warning: {warning}"); }
    Ok(())
}

fn validate(input: PathBuf, warnings_as_errors: bool) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    if warnings_as_errors && !dataset.warnings.is_empty() {
        return Err(format!("validation warnings are errors: {}", dataset.warnings.join("; ")).into());
    }
    let projection = calculate_projection(&dataset, &ProjectionOptions::default())?;
    println!(
        "Valid: {} grids, {} phases, {} isotherm paths, {} invariants, {} univariants",
        dataset.grids.len(), dataset.phases.len(), projection.diagnostics.contour_path_count,
        projection.diagnostics.invariant_count, projection.diagnostics.univariant_count,
    );
    Ok(())
}

fn plot(input: PathBuf, output: PathBuf, options: PlotOptions) -> Result<(), Box<dyn Error>> {
    let dataset = parse_path(&input)?;
    let levels = options.levels.as_deref().map(parse_level_spec).transpose()?;
    let projection = calculate_projection(&dataset, &ProjectionOptions {
        levels: levels.unwrap_or_default(),
        sampling_subdivisions: options.sampling_subdivisions,
        regularize: options.regularize && !options.no_regularize,
    })?;
    let selected_layers = options.show_isotherms || options.show_univariants || options.show_invariants || options.show_binary_invariants;
    let mut render = RenderOptions::default();
    if selected_layers {
        render.show_isotherms = options.show_isotherms;
        render.show_univariants = options.show_univariants;
        render.show_invariants = options.show_invariants;
        render.show_binary_invariants = options.show_binary_invariants;
    }
    render.show_grid = options.show_grid;
    render.show_samples = options.show_samples;
    render.width = options.width.unwrap_or(render.width);
    render.height = options.height.unwrap_or(render.height);
    render.title = options.title;
    render_to_path(&output, &dataset, &projection, &render, options.format.map(Into::into))?;
    println!(
        "Wrote {} ({} isotherm paths, {} invariants, {} univariants)",
        output.display(), projection.diagnostics.contour_path_count,
        projection.diagnostics.invariant_count, projection.diagnostics.univariant_count,
    );
    Ok(())
}
