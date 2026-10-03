# Try rnx in a terminal

Three short scripts, each a single command, with no dependencies beyond `rnx` itself and
nothing fetched from the network. Each prints its answer, and `tests/demos.rs` checks that
answer against an independent computation.

## Setup (once)

`cargo install` installs the executable, not this `demos/` directory, so get both from a
checkout. Rust, Git and native build tools are required:

```sh
git clone https://github.com/nilket-com/rnx
cd rnx
cargo install --path . --locked
```

Run the demos from the checkout's root.

## 1. A useful calculation

```sh
rnx run demos/01_mortgage.rn
```

What a 30-year mortgage costs, in about forty lines: functions, a loop and formatted numbers,
with no imports.

```text
Loan 300,000.00 at 6.0% a year for 30 years, paid monthly
(a constant rate; totals exclude fees, taxes and insurance)
Monthly payment       1,798.65
(totals use the unrounded payment, shown here to four decimals: 1798.6516)
Total paid          647,514.57
Total interest      347,514.57
```

## 2. JSON in, answers out

```sh
rnx run demos/02_orders.rn
```

It reads [`data/orders.json`](data/orders.json) and answers three questions about it: which
regions earn, who buys most, and what is still open. Money stays in integer cents, so the
one-cent lead below is exact.

```text
Shipped revenue by region:
  north    245.00
  east     144.97
  south     99.95
  west       0.00
Largest customer: Acme (134.99), ahead of Birch (134.98)
Still open: 4 of 12 orders (33.3%)
```

Revenue counts shipped orders only. The open share is out of all orders.

## 3. A readable report

```sh
rnx run demos/03_report.rn
```

The same orders as an aligned terminal table, built from ordinary format strings such as
`{:>10}` and `{:>7.1}`, with no table library.

```text
region   orders  units   revenue   share
--------------------------------------------------------------
east          3     11    144.97   29.6%  ############
north         3     13    245.00   50.0%  ####################
south         4      5     99.95   20.4%  ########
west          2      0      0.00    0.0%
--------------------------------------------------------------
total        12     29    489.92  100.0%
```

Each region's order count covers every status. Units and revenue count shipped orders only.

Either order demo takes another file of the same shape as its first argument:
`rnx run demos/02_orders.rn path/to/orders.json`. An empty JSON array, a file with a single
customer, or one with nothing shipped yet is reported plainly. With no shipped revenue the
shares are undefined, shown as `-`, and there are no bars.

## The same demos as notebooks

[`notebooks/`](notebooks/README.md) has the three demos as short Jupyter walkthroughs on rnx's own
kernel (Linux only), each checked to print exactly what its terminal demo prints, plus a Polars
notebook: a short analysis of a bundled CSV, checked against the data.
