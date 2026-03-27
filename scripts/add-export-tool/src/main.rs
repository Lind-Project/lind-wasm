use anyhow::{bail, Context, Result};
use std::{env, fs};
use wasm_encoder::{ExportKind, ExportSection, Module, RawSection};
use wasmparser::{ExternalKind, KnownCustom, Name, Parser, Payload};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetKind {
    Func,
    Global,
}

#[derive(Clone, Copy)]
enum EmittedSection<'a> {
    Raw { id: u8, data: &'a [u8] },
    NewExport,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);

    let input = args.next().context(
        "usage: add-wasm-export <in.wasm> <out.wasm> <export-name> <func|global> <target-name>",
    )?;
    let output = args.next().context(
        "usage: add-wasm-export <in.wasm> <out.wasm> <export-name> <func|global> <target-name>",
    )?;
    let export_name = args.next().context(
        "usage: add-wasm-export <in.wasm> <out.wasm> <export-name> <func|global> <target-name>",
    )?;
    let kind = parse_target_kind(&args.next().context(
        "usage: add-wasm-export <in.wasm> <out.wasm> <export-name> <func|global> <target-name>",
    )?)?;
    let target_name = args.next().context(
        "usage: add-wasm-export <in.wasm> <out.wasm> <export-name> <func|global> <target-name>",
    )?;

    let wasm = fs::read(&input).with_context(|| format!("failed to read {input}"))?;

    let target_index = find_named_target(&wasm, kind, &target_name)
        .with_context(|| format!("could not find {kind:?} named {target_name:?}"))?;

    let rewritten = add_export(&wasm, &export_name, kind, target_index)?;
    wasmparser::validate(&rewritten).context("rewritten wasm is invalid")?;

    fs::write(&output, rewritten).with_context(|| format!("failed to write {output}"))?;

    println!(
        "added export {:?} -> {:?} {:?} (index {})",
        export_name, kind, target_name, target_index
    );

    Ok(())
}

fn parse_target_kind(s: &str) -> Result<TargetKind> {
    match s {
        "func" | "function" => Ok(TargetKind::Func),
        "global" => Ok(TargetKind::Global),
        _ => bail!("kind must be 'func' or 'global'"),
    }
}

fn export_kind(kind: TargetKind) -> ExportKind {
    match kind {
        TargetKind::Func => ExportKind::Func,
        TargetKind::Global => ExportKind::Global,
    }
}

fn map_external_kind(kind: ExternalKind) -> Option<ExportKind> {
    match kind {
        ExternalKind::Func => Some(ExportKind::Func),
        ExternalKind::Table => Some(ExportKind::Table),
        ExternalKind::Memory => Some(ExportKind::Memory),
        ExternalKind::Global => Some(ExportKind::Global),
        ExternalKind::Tag => Some(ExportKind::Tag),
        _ => None,
    }
}

fn find_named_target(wasm: &[u8], wanted_kind: TargetKind, wanted_name: &str) -> Result<u32> {
    for payload in Parser::new(0).parse_all(wasm) {
        match payload? {
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(names) = section.as_known() {
                    for subsection in names {
                        match subsection? {
                            Name::Function(map) if wanted_kind == TargetKind::Func => {
                                for naming in map {
                                    let naming = naming?;
                                    if naming.name == wanted_name {
                                        return Ok(naming.index);
                                    }
                                }
                            }
                            Name::Global(map) if wanted_kind == TargetKind::Global => {
                                for naming in map {
                                    let naming = naming?;
                                    if naming.name == wanted_name {
                                        return Ok(naming.index);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    bail!("target name not found in name section")
}

fn add_export(
    wasm: &[u8],
    export_name: &str,
    new_kind: TargetKind,
    new_index: u32,
) -> Result<Vec<u8>> {
    let mut emitted: Vec<EmittedSection<'_>> = Vec::new();

    let mut saw_export = false;
    let mut inserted_new_export = false;

    let mut rebuilt_exports = ExportSection::new();
    let mut existing_export_name_present = false;

    let mut code_section_start: Option<usize> = None;
    let mut code_section_end: Option<usize> = None;

    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload?;

        if !saw_export && !inserted_new_export && should_insert_export_before(&payload) {
            emitted.push(EmittedSection::NewExport);
            inserted_new_export = true;
        }

        match payload {
            Payload::Version { .. } | Payload::End(_) => {}

            Payload::TypeSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Type.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::ImportSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Import.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::FunctionSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Function.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::TableSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Table.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::MemorySection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Memory.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::GlobalSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Global.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::ExportSection(s) => {
                saw_export = true;

                for export in s {
                    let export = export?;
                    let kind = map_external_kind(export.kind)
                        .context("unsupported existing export kind")?;

                    if export.name == export_name {
                        existing_export_name_present = true;
                    }

                    rebuilt_exports.export(export.name, kind, export.index);
                }

                if !existing_export_name_present {
                    rebuilt_exports.export(export_name, export_kind(new_kind), new_index);
                }

                emitted.push(EmittedSection::NewExport);
            }

            Payload::StartSection { range, .. } => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Start.into(),
                    &wasm[range.start..range.end],
                ));
            }

            Payload::ElementSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Element.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::DataCountSection { range, .. } => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::DataCount.into(),
                    &wasm[range.start..range.end],
                ));
            }

            Payload::CodeSectionStart { range, .. } => {
                code_section_start = Some(range.start);
            }

            Payload::CodeSectionEntry(body) => {
                code_section_end = Some(body.range().end);
            }

            Payload::DataSection(s) => {
                // Flush code section before data, because code must stay before data.
                if let (Some(start), Some(end)) =
                    (code_section_start.take(), code_section_end.take())
                {
                    emitted.push(raw_section(
                        wasm_encoder::SectionId::Code.into(),
                        &wasm[start..end],
                    ));
                }

                emitted.push(raw_section(
                    wasm_encoder::SectionId::Data.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::TagSection(s) => {
                // Flush code section before tag too, in case tag comes after code/data in this file.
                if let (Some(start), Some(end)) =
                    (code_section_start.take(), code_section_end.take())
                {
                    emitted.push(raw_section(
                        wasm_encoder::SectionId::Code.into(),
                        &wasm[start..end],
                    ));
                }

                emitted.push(raw_section(
                    wasm_encoder::SectionId::Tag.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            Payload::CustomSection(s) => {
                emitted.push(raw_section(
                    wasm_encoder::SectionId::Custom.into(),
                    &wasm[s.range().start..s.range().end],
                ));
            }

            _ => {}
        }
    }

    // Flush code section if it was present and not yet emitted.
    if let (Some(start), Some(end)) = (code_section_start.take(), code_section_end.take()) {
        emitted.push(raw_section(
            wasm_encoder::SectionId::Code.into(),
            &wasm[start..end],
        ));
    }

    if !saw_export && !inserted_new_export {
        let mut exports = ExportSection::new();
        exports.export(export_name, export_kind(new_kind), new_index);
        rebuilt_exports = exports;
        emitted.push(EmittedSection::NewExport);
    }

    let mut module = Module::new();

    for section in emitted {
        match section {
            EmittedSection::Raw { id, data } => {
                module.section(&RawSection { id, data });
            }
            EmittedSection::NewExport => {
                module.section(&rebuilt_exports);
            }
        }
    }

    Ok(module.finish())
}

fn raw_section<'a>(id: u8, data: &'a [u8]) -> EmittedSection<'a> {
    EmittedSection::Raw { id, data }
}

fn should_insert_export_before(payload: &Payload<'_>) -> bool {
    matches!(
        payload,
        Payload::StartSection { .. }
            | Payload::ElementSection(_)
            | Payload::DataCountSection { .. }
            | Payload::CodeSectionStart { .. }
            | Payload::DataSection(_)
            | Payload::TagSection(_)
    )
}
