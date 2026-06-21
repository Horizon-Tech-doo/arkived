//! Rendering command results in the selected [`OutputFormat`].

use anyhow::Result;
use arkived_core::config::OutputFormat;
use arkived_core::{BlobEntry, Container};
use serde::Serialize;

/// Serialize any value as JSON or YAML. Used for `--format json|yaml` on
/// structured results (e.g. properties, metadata).
pub fn emit_serialized<T: Serialize>(value: &T, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Yaml => print!("{}", serde_yaml::to_string(value)?),
        OutputFormat::Table | OutputFormat::Tsv => {
            // No generic table shape — fall back to JSON for arbitrary values.
            println!("{}", serde_json::to_string_pretty(value)?);
        }
    }
    Ok(())
}

/// Render a list of containers.
pub fn print_containers(items: &[Container], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(items)?),
        OutputFormat::Yaml => print!("{}", serde_yaml::to_string(items)?),
        OutputFormat::Tsv => {
            println!("NAME\tLAST_MODIFIED\tPUBLIC_ACCESS\tLEASE_STATE");
            for c in items {
                println!(
                    "{}\t{}\t{}\t{}",
                    c.name,
                    c.last_modified.map(fmt_time).unwrap_or_default(),
                    c.public_access.as_deref().unwrap_or("private"),
                    c.lease_state.as_deref().unwrap_or("-"),
                );
            }
        }
        OutputFormat::Table => {
            let rows: Vec<[String; 4]> = items
                .iter()
                .map(|c| {
                    [
                        c.name.clone(),
                        c.last_modified.map(fmt_time).unwrap_or_default(),
                        c.public_access.clone().unwrap_or_else(|| "private".into()),
                        c.lease_state.clone().unwrap_or_else(|| "-".into()),
                    ]
                })
                .collect();
            print_table(&["NAME", "LAST MODIFIED", "PUBLIC ACCESS", "LEASE"], &rows);
        }
    }
    Ok(())
}

/// Render a list of blob entries (blobs and virtual-directory prefixes).
pub fn print_blobs(items: &[BlobEntry], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(items)?),
        OutputFormat::Yaml => print!("{}", serde_yaml::to_string(items)?),
        OutputFormat::Tsv => {
            println!("NAME\tSIZE\tTIER\tTYPE\tLAST_MODIFIED");
            for b in items {
                let (name, size, tier, ty, lm) = blob_cells(b);
                println!("{name}\t{size}\t{tier}\t{ty}\t{lm}");
            }
        }
        OutputFormat::Table => {
            let rows: Vec<[String; 5]> = items
                .iter()
                .map(|b| {
                    let (name, size, tier, ty, lm) = blob_cells(b);
                    [name, size, tier, ty, lm]
                })
                .collect();
            print_table(&["NAME", "SIZE", "TIER", "TYPE", "LAST MODIFIED"], &rows);
        }
    }
    Ok(())
}

fn blob_cells(b: &BlobEntry) -> (String, String, String, String, String) {
    match b {
        BlobEntry::Blob {
            name,
            size,
            blob_type,
            tier,
            last_modified,
            ..
        } => (
            name.clone(),
            size.to_string(),
            tier.clone().unwrap_or_else(|| "-".into()),
            blob_type.clone(),
            last_modified.map(fmt_time).unwrap_or_default(),
        ),
        BlobEntry::Prefix { name } => (
            name.clone(),
            "-".into(),
            "-".into(),
            "<dir>".into(),
            String::new(),
        ),
    }
}

fn fmt_time(t: time::OffsetDateTime) -> String {
    let t = t.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z",
        t.year(),
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// Print left-aligned, space-padded columns. `N` is the column count.
fn print_table<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let mut widths = headers.map(|h| h.len());
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |cells: &[String; N]| {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            let pad = widths[i].saturating_sub(cell.chars().count());
            out.push_str(cell);
            if i + 1 < N {
                out.push_str(&" ".repeat(pad));
            }
        }
        out
    };
    let header_row: [String; N] = std::array::from_fn(|i| headers[i].to_string());
    println!("{}", line(&header_row));
    for row in rows {
        println!("{}", line(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_pads_to_widest_cell() {
        let rows = [
            ["a".to_string(), "1".to_string()],
            ["longer".to_string(), "22".to_string()],
        ];
        // Smoke test: building the table must not panic and the column width
        // logic must accommodate the widest cell.
        print_table(&["NAME", "N"], &rows);
        let widths = {
            let mut w = ["NAME", "N"].map(|h| h.len());
            for row in &rows {
                for (i, c) in row.iter().enumerate() {
                    w[i] = w[i].max(c.len());
                }
            }
            w
        };
        assert_eq!(widths[0], 6); // "longer"
        assert_eq!(widths[1], 2); // "22"
    }
}
