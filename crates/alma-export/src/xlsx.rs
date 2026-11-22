//! Tables → XLSX workbook with `rust_xlsxwriter` (SPEC.md §5 M2, ADR 0016). The workbook is
//! deterministic: document properties carry a fixed creation date, so the same tables give the
//! same bytes.

use rust_xlsxwriter::{DocProperties, ExcelDateTime, Format, Workbook, XlsxError};

/// One cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Text.
    Text(String),
    /// A number shown with two decimals and thousands separators.
    Money(f64),
    /// A plain number.
    Number(f64),
    /// A date, `YYYY-MM-DD`.
    Date(String),
    /// Nothing.
    Empty,
}

/// One worksheet: a title line, a header row and data rows. Rows whose first cell is in
/// `bold_rows` (e.g. "Total") are bold.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sheet {
    /// Worksheet name (at most 31 characters).
    pub name: String,
    /// Lines written above the table (project, currency, basis).
    pub notes: Vec<String>,
    /// Column headers.
    pub header: Vec<String>,
    /// Data rows.
    pub rows: Vec<Vec<Cell>>,
    /// Labels in the first column that make a row bold.
    pub bold_rows: Vec<String>,
}

/// Writes sheets into an XLSX file in memory.
pub fn to_xlsx(title: &str, sheets: &[Sheet]) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let fixed = ExcelDateTime::from_ymd(2000, 1, 1)?;
    wb.set_properties(
        &DocProperties::new()
            .set_title(title)
            .set_author("alma")
            .set_creation_datetime(&fixed),
    );
    let bold = Format::new().set_bold();
    let money = Format::new().set_num_format("#,##0.00");
    let money_bold = Format::new().set_num_format("#,##0.00").set_bold();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    for sheet in sheets {
        let ws = wb.add_worksheet();
        ws.set_name(&sheet.name)?;
        let mut row: u32 = 0;
        for note in &sheet.notes {
            ws.write_string(row, 0, note)?;
            row += 1;
        }
        if !sheet.notes.is_empty() {
            row += 1;
        }
        for (c, h) in sheet.header.iter().enumerate() {
            ws.write_string_with_format(row, c as u16, h, &bold)?;
        }
        let header_row = row;
        row += 1;
        for cells in &sheet.rows {
            let strong =
                matches!(cells.first(), Some(Cell::Text(t)) if sheet.bold_rows.contains(t));
            for (c, cell) in cells.iter().enumerate() {
                let c = c as u16;
                match cell {
                    Cell::Text(t) if strong => {
                        ws.write_string_with_format(row, c, t, &bold)?;
                    }
                    Cell::Text(t) => {
                        ws.write_string(row, c, t)?;
                    }
                    Cell::Money(v) => {
                        let f = if strong { &money_bold } else { &money };
                        ws.write_number_with_format(row, c, *v, f)?;
                    }
                    Cell::Number(v) => {
                        ws.write_number(row, c, *v)?;
                    }
                    Cell::Date(d) => match ExcelDateTime::parse_from_str(d) {
                        Ok(dt) => {
                            ws.write_datetime_with_format(row, c, &dt, &date)?;
                        }
                        Err(_) => {
                            ws.write_string(row, c, d)?;
                        }
                    },
                    Cell::Empty => {}
                }
            }
            row += 1;
        }
        let widths: Vec<usize> = (0..sheet.header.len())
            .map(|c| {
                let body = sheet
                    .rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .map(|cell| match cell {
                        Cell::Text(t) => t.chars().count(),
                        Cell::Money(_) | Cell::Number(_) => 12,
                        Cell::Date(_) => 10,
                        Cell::Empty => 0,
                    })
                    .max()
                    .unwrap_or(0);
                body.max(sheet.header[c].chars().count()).clamp(8, 48)
            })
            .collect();
        for (c, w) in widths.iter().enumerate() {
            ws.set_column_width(c as u16, *w as f64 + 2.0)?;
        }
        ws.set_freeze_panes(header_row + 1, 1)?;
    }
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Sheet {
        Sheet {
            name: "Cost per period".into(),
            notes: vec!["Project: test".into()],
            header: vec!["Item".into(), "2027-01".into()],
            rows: vec![
                vec![Cell::Text("dev".into()), Cell::Money(1234.5)],
                vec![Cell::Text("Total".into()), Cell::Money(1234.5)],
                vec![Cell::Date("2027-01-04".into()), Cell::Empty],
            ],
            bold_rows: vec!["Total".into()],
        }
    }

    #[test]
    fn writes_a_zip_and_is_deterministic() {
        let a = to_xlsx("t", &[sheet()]).unwrap();
        let b = to_xlsx("t", &[sheet()]).unwrap();
        assert_eq!(&a[..2], b"PK");
        assert_eq!(a, b, "same tables, same bytes");
    }
}
