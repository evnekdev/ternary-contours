use std::{error::Error, path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use ternary_contours_cli::{
    IrregularTemplateStyle, NumericFormat, OutputFormat, ProjectionOptions, RenderOptions,
    TabulatedGrid, calculate_projection, compositions_tsv, irregular_template, parse_components,
    parse_field_specs, parse_level_spec, parse_path, regular_template_tct, render_to_path,
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
                        defined + values.clone().filter(|value| value.is_calculated()).count(),
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

fn write_generated(output: Option<PathBuf>, content: String) -> Result<(), Box<dyn Error>> {
    if let Some(output) = output {
        std::fs::write(&output, content)?;
        println!("Wrote {}", output.display());
    } else {
        print!("{content}");
    }
    Ok(())
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
