# Funder

A terminal application for tracking the cost basis of appreciated shares donated to a
donor-advised fund. Purchases are recorded as whole lots; a donation draws on the lots with the
highest long-term gain, and the app shows each donation's cost basis and the gain it avoids. It
replaces a personal spreadsheet.

## Usage

```bash
cargo install --path .
funder                      # opens ~/.local/share/funder/funder.db
funder --db /tmp/demo.db    # a scratch database
funder --scratch            # a throwaway copy of the default database
```

`1` shows the Lots: every purchase, what is left of it, and its gain at the price `p` sets. `a`, `e`,
and `d` add, edit, and delete a lot; `?` lists every key.

`2` shows the Donations. `n` plans one from a dollar target, drawing on the long-term lots with the
highest gain; `r` records it once the shares have moved; `c` marks it claimed.

## No real data in the repository

This repository is public and the owner's holdings are not. Nothing committed here carries a real
amount, ticker, institution, or name, and test fixtures use invented figures. `AGENTS.md` states
the rule in full.

## License

[MIT](LICENSE)
