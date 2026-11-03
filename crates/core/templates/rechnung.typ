// Rechnungsvorlage. Die Daten kommen aus data.typ (wird vom Programm erzeugt) als `invoice`.
// Eigene Anpassungen: Datei nach <Datenordner>/vorlagen/rechnung.typ kopieren und dort ändern.
#import "data.typ": invoice

#let s = invoice.sender
#let lines(items) = items.filter(x => x != "").join(linebreak())

#set page(
  paper: "a4",
  margin: (top: 20mm, bottom: 38mm, left: 25mm, right: 20mm),
  footer: [
    #set text(size: 8pt, fill: luma(80))
    #line(length: 100%, stroke: 0.4pt + luma(160))
    #v(2mm)
    #grid(
      columns: (1fr, 1fr),
      [#lines((s.name, s.street, s.zip_city, if s.email != "" { "E-Mail: " + s.email } else { "" }))],
      [#lines((
        if s.bank_name != "" { "Kreditinstitut: " + s.bank_name } else { "" },
        if s.iban != "" { "IBAN: " + s.iban } else { "" },
        if s.bic != "" { "BIC: " + s.bic } else { "" },
        if s.account_holder != "" { "Kto. Inh.: " + s.account_holder } else { "" },
      ))],
    )
  ],
)
#set text(font: ("Arial", "Liberation Sans", "Libertinus Serif"), size: 10pt, lang: "de")
#set par(justify: false, leading: 0.6em)

#grid(
  columns: (1fr, 1fr),
  column-gutter: 10mm,
  [#lines(invoice.recipient)],
  align(right)[
    #lines((
      s.name,
      s.street,
      s.zip_city,
      if s.tax_id != "" { "Steueridentifikationsnummer: " + s.tax_id } else { "" },
      if s.income_tax_no != "" { "Einkommensteuernummer: " + s.income_tax_no } else { "" },
    ))
  ],
)

#v(12mm)
#align(right)[
  Datum: #invoice.date \
  Rechnungsnummer: #invoice.number \
  #text(size: 8.5pt)[Rechnungsdatum entspricht Liefer-/Leistungsdatum]
]

#v(6mm)
#text(size: 15pt, weight: "bold")[#invoice.title]
#if invoice.reference != "" [ \ #invoice.reference ]

#v(4mm)
#invoice.salutation

#invoice.intro

#v(2mm)
#table(
  columns: (auto, auto, auto, 1fr, auto, auto),
  align: (left, right, left, left, right, right),
  stroke: none,
  inset: (x: 4pt, y: 5pt),
  table.hline(stroke: 0.6pt),
  table.header([*Pos.*], [*Anzahl*], [*Einheit*], [*Bezeichnung*], [*Einzelpreis*], [*Gesamtpreis*]),
  table.hline(stroke: 0.6pt),
  ..invoice.lines.map(l => (
    [#l.pos], [#l.qty], [#l.unit], [#l.desc.split("\n").join(linebreak())], [#l.price], [#l.total],
  )).flatten(),
  table.hline(stroke: 0.6pt),
  table.cell(colspan: 5, align: right)[*Rechnungsbetrag*],
  [*#invoice.total*],
)

#v(6mm)
#if invoice.payment_text != "" [#invoice.payment_text]

#if invoice.footer != "" [
  #v(2mm)
  #text(size: 9pt)[#invoice.footer]
]

#v(6mm)
Mit freundlichen Grüßen \
#s.name
