//! Rechnungs-PDF: Daten werden als Typst-Quelltext (`data.typ`) erzeugt und mit der Vorlage
//! `templates/rechnung.typ` (oder einer eigenen Kopie im Datenordner) gesetzt.

use typst_as_lib::{typst_kit_options::TypstKitFontOptions, TypstEngine};

use crate::{
    date::format_de,
    error::{Error, Result},
    finance::EuerReport,
    invoicing::Invoice,
    money::format_eur,
    settings::Sender,
};

pub const DEFAULT_TEMPLATE: &str = include_str!("../templates/rechnung.typ");
pub const EUER_TEMPLATE: &str = include_str!("../templates/euer.typ");

/// Typst-Zeichenkettenliteral.
fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

/// Typst-Array (mit abschließendem Komma, damit auch ein einzelnes Element ein Array bleibt).
fn array(items: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("(");
    for item in items {
        out.push_str(&item);
        out.push(',');
    }
    out.push(')');
    out
}

/// Einleitung und Zahlungshinweis eines Belegs (`storno_of` = Nummer der stornierten Rechnung).
/// Auch die Vorschau in der Oberfläche nutzt diese Texte.
pub fn letter_texts(storno_of: Option<&str>, payment_days: i64) -> (String, String) {
    match storno_of {
        Some(n) => (
            format!("hiermit stornieren wir die Rechnung {n}. Folgende Positionen werden gutgeschrieben:"),
            String::new(),
        ),
        None => (
            "vielen Dank für Ihren Auftrag. Vereinbarungsgemäß berechnen wir Ihnen hiermit folgende Leistungen:"
                .to_string(),
            format!(
                "Bitte überweisen Sie den Rechnungsbetrag innerhalb von {payment_days} Tagen auf unser unten genanntes \
                 Konto. Für weitere Fragen stehen wir Ihnen sehr gerne zur Verfügung."
            ),
        ),
    }
}

fn data_source(inv: &Invoice, sender: &Sender, storno_of: Option<&str>) -> String {
    let title = if storno_of.is_some() { "Stornorechnung" } else { "Rechnung" };
    let reference = storno_of.map(|n| format!("Storno zu Rechnung {n}")).unwrap_or_default();
    let (intro, payment_text) = letter_texts(storno_of, sender.payment_days);

    let lines = array(inv.lines.iter().enumerate().map(|(i, l)| {
        format!(
            "(pos: {}, desc: {}, unit: {}, qty: {}, price: {}, total: {},)",
            i + 1,
            q(&l.description),
            q(&l.unit),
            q(&l.qty.to_string()),
            q(&format_eur(l.unit_price_cents)),
            q(&format_eur(l.total_cents())),
        )
    }));
    let recipient = array(inv.recipient.lines().map(q));
    let sender_dict = format!(
        "(name: {}, street: {}, zip_city: {}, email: {}, tax_id: {}, income_tax_no: {}, \
         bank_name: {}, account_holder: {}, iban: {}, bic: {},)",
        q(&sender.name),
        q(&sender.street),
        q(&sender.zip_city),
        q(&sender.email),
        q(&sender.tax_id),
        q(&sender.income_tax_no),
        q(&sender.bank_name),
        q(&sender.account_holder),
        q(&sender.iban),
        q(&sender.bic),
    );

    format!(
        "#let invoice = (\n  number: {},\n  date: {},\n  title: {},\n  reference: {},\n  salutation: {},\n  \
         intro: {},\n  recipient: {},\n  lines: {},\n  total: {},\n  payment_text: {},\n  footer: {},\n  \
         sender: {},\n)\n",
        q(&inv.number),
        q(&format_de(&inv.issued_at)),
        q(title),
        q(&reference),
        q(&inv.salutation),
        q(&intro),
        recipient,
        lines,
        q(&format_eur(inv.total_cents)),
        q(&payment_text),
        q(&inv.footer),
        sender_dict,
    )
}

macro_rules! pdf_error {
    () => {
        |errs| {
            Error::Invalid(format!(
                "PDF-Erstellung fehlgeschlagen: {}",
                errs.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("; ")
            ))
        }
    };
}

/// Erzeugt das PDF. `storno_of` ist die Nummer der stornierten Rechnung (dann Stornobeleg).
pub fn render_invoice(inv: &Invoice, sender: &Sender, storno_of: Option<&str>, template: &str) -> Result<Vec<u8>> {
    compile_pdf(template, data_source(inv, sender, storno_of))
}

fn report_source(r: &EuerReport, sender: &Sender) -> String {
    let categories = array(r.by_category.iter().map(|c| {
        let label = if c.euer_label.is_empty() { &c.category } else { &c.euer_label };
        format!(
            "(line: {}, label: {}, category: {}, count: {}, total: {},)",
            q(&c.euer_line),
            q(label),
            q(&c.category),
            c.count,
            q(&format_eur(-c.total_cents)),
        )
    }));
    let income = array(r.income.iter().map(|i| {
        format!(
            "(date: {}, number: {}, customer: {}, amount: {},)",
            q(&format_de(&i.paid_at)),
            q(&i.invoice_number),
            q(&i.customer),
            q(&format_eur(i.amount_cents)),
        )
    }));
    // Ausgaben chronologisch aufsteigend
    let expenses = array(r.expenses.iter().rev().map(|e| {
        let x = &e.expense;
        format!(
            "(date: {}, qty: {}, desc: {}, vendor: {}, category: {}, amount: {}, receipt: {},)",
            q(&format_de(&x.spent_at)),
            q(&x.qty.to_string()),
            q(&x.description),
            q(&x.vendor),
            q(&e.category),
            q(&format_eur(x.amount_cents)),
            q(x.receipt_label()),
        )
    }));
    format!(
        "#let report = (\n  year: {},\n  income_line: {},\n  income_label: {},\n  income_total: {},\n  \
         expense_total: {},\n  profit: {},\n  missing: {},\n  categories: {},\n  income: {},\n  expenses: {},\n  \
         sender: (name: {}, tax_id: {}, income_tax_no: {},),\n)\n",
        r.year,
        q(&r.income_line),
        q(&r.income_label),
        q(&format_eur(r.income_total_cents)),
        q(&format_eur(r.expense_total_cents)),
        q(&format_eur(r.profit_cents)),
        r.missing_receipts,
        categories,
        income,
        expenses,
        q(&sender.name),
        q(&sender.tax_id),
        q(&sender.income_tax_no),
    )
}

/// Jahresauswertung (EÜR) als PDF.
pub fn render_euer(report: &EuerReport, sender: &Sender, template: &str) -> Result<Vec<u8>> {
    compile_pdf(template, report_source(report, sender))
}

/// Setzt `main.typ` (Vorlage) mit den erzeugten Daten (`data.typ`) zu einem PDF.
fn compile_pdf(template: &str, data: String) -> Result<Vec<u8>> {
    let engine = TypstEngine::builder()
        .search_fonts_with(TypstKitFontOptions::default())
        .with_static_source_file_resolver([("main.typ", template.to_string()), ("data.typ", data)])
        .build();
    let doc = engine
        .compile("main.typ")
        .output
        .map_err(|e| Error::Invalid(format!("PDF-Erstellung fehlgeschlagen: {e}")))?;
    typst_pdf::pdf(&doc, &Default::default()).map_err(pdf_error!())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_typst_strings() {
        assert_eq!(q("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn arrays_keep_single_element() {
        assert_eq!(array(["\"x\"".to_string()]), "(\"x\",)");
        assert_eq!(array(Vec::<String>::new()), "()");
    }
}
