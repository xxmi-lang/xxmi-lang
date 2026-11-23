//! Which command-list keys bind mesh data (port of packages/core/src/model/mesh.ts).

use super::{IniFile, Reference};

/// `vb0`, `vb1`, ... as the DLL dispatches on them (lower-cased key).
fn vertex_buffer_slot(command: &str) -> Option<u32> {
    let digits = command.strip_prefix("vb")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A key that binds any vertex buffer slot or the index buffer.
pub fn is_mesh_slot(command: &str) -> bool {
    command == "ib" || vertex_buffer_slot(command).is_some()
}

/// A key that binds the slots a mesh needs to draw: `vb0` to `vb2` and `ib`.
pub fn is_core_mesh_slot(command: &str) -> bool {
    command == "ib" || vertex_buffer_slot(command).is_some_and(|slot| slot <= 2)
}

/// Whether `reference` is written on a line that binds a core mesh slot (`vb0 = ref ResourceX`).
pub fn binds_core_mesh_slot(file: &IniFile, reference: &Reference) -> bool {
    let Some(section) = reference.section.and_then(|index| file.sections.get(index)) else {
        return false;
    };

    section
        .lines
        .iter()
        .find(|line| {
            let content = line.scanned.content;
            content.start <= reference.span.start && reference.span.end <= content.end
        })
        .and_then(|line| line.command.as_deref())
        .is_some_and(is_core_mesh_slot)
}
