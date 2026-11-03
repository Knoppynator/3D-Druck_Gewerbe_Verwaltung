// Vorlage für die Jahresauswertung (EÜR). Die Daten kommen aus data.typ (wird vom Programm erzeugt) als `report`.
// Eigene Anpassungen: Datei nach <Datenordner>/vorlagen/euer.typ kopieren und dort ändern.
#import "data.typ": report

#set page(
  paper: "a4",
  margin: (top: 20mm, bottom: 20mm, left: 20mm, right: 20mm),
  footer: context [
    #set text(size: 8pt, fill: luma(100))
    #report.sender.name · Einnahmen-Überschuss-Rechnung #report.year #h(1fr) Seite #counter(page).display() von #counter(page).final().first()
  ],
)
#set text(font: ("Arial", "Liberation Sans", "Libertinus Serif"), size: 9.5pt, lang: "de")
#set par(justify: false)

#let right-cell(body) = align(right, body)

= Einnahmen-Überschuss-Rechnung #report.year

#report.sender.name \
#if report.sender.tax_id != "" [Steueridentifikationsnummer: #report.sender.tax_id \ ]
#if report.sender.income_tax_no != "" [Einkommensteuernummer: #report.sender.income_tax_no]

== Zusammenfassung

#table(
  columns: (auto, 1fr, auto),
  align: (left, left, right),
  stroke: none,
  inset: (x: 4pt, y: 4pt),
  table.hline(stroke: 0.6pt),
  table.header([*Zeile*], [*Position*], [*Betrag*]),
  table.hline(stroke: 0.6pt),
  [#report.income_line], [#report.income_label], [#report.income_total],
  ..report.categories.map(c => (
    [#c.line],
    [#c.label #text(fill: luma(110))[(#c.category, #c.count Buchungen)]],
    [#c.total],
  )).flatten(),
  table.hline(stroke: 0.6pt),
  [], [*Summe Betriebsausgaben*], [*#report.expense_total*],
  [], [*Gewinn / Verlust*], [*#report.profit*],
)

#text(size: 8pt, fill: luma(100))[
  Einnahmen nach Zahlungsdatum (Zuflussprinzip). Die Zeilennummern entsprechen den in der Software gepflegten
  Zuordnungen und sind vor der Übertragung in ELSTER anhand des aktuellen Formulars zu prüfen.
]

#if report.missing > 0 [
  #v(2mm)
  #text(fill: rgb("#b42318"))[*Hinweis:* Zu #report.missing Ausgaben fehlt der Beleg.]
]

== Einnahmen

#table(
  columns: (auto, auto, 1fr, auto),
  align: (left, left, left, right),
  stroke: none,
  inset: (x: 4pt, y: 3pt),
  table.hline(stroke: 0.6pt),
  table.header([*Zahlung*], [*Rechnung*], [*Kunde*], [*Betrag*]),
  table.hline(stroke: 0.6pt),
  ..report.income.map(i => ([#i.date], [#i.number], [#i.customer], [#i.amount])).flatten(),
  table.hline(stroke: 0.6pt),
  [], [], [*Summe*], [*#report.income_total*],
)

== Ausgaben

#table(
  columns: (auto, auto, 1fr, auto, auto, auto),
  align: (left, right, left, left, right, left),
  stroke: none,
  inset: (x: 4pt, y: 3pt),
  table.hline(stroke: 0.6pt),
  table.header([*Datum*], [*Menge*], [*Bezeichnung*], [*Kategorie*], [*Betrag*], [*Beleg*]),
  table.hline(stroke: 0.6pt),
  ..report.expenses.map(e => (
    [#e.date], [#e.qty], [#e.desc #if e.vendor != "" [#text(fill: luma(110))[(#e.vendor)]]], [#e.category], [#e.amount], [#e.receipt],
  )).flatten(),
  table.hline(stroke: 0.6pt),
  [], [], [], [*Summe*], [*#report.expense_total*], [],
)
