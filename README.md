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
funder backup               # upload the database to S3 if a backup is due
```

`1` shows the Lots: every purchase, what is left of it, and its gain at the price `p` sets. `a`, `e`,
and `d` add, edit, and delete a lot; `?` lists every key.

`2` shows the Donations. `n` plans one from a dollar target, drawing on the long-term lots with the
highest gain; `r` records it once the shares have moved; `c` marks it claimed.
`o` lists every lot a donation could draw on, short-term and losing ones in red, and takes the
shares typed against each; those choices survive recording. `A` restores the automatic choice.

## Backups

`funder` uploads a copy of the database to S3 when the last upload is older than `interval_days`.
The check runs after the screen is torn down, so a slow network only holds up the prompt, and a
failure prints to stderr without failing the run. A run given `--db` or `--scratch` skips it: the
state file records when a backup last ran, not which file. `--today` does not skip it, since the
schedule reads the real clock.

Off until a config file switches it on:

```toml
# ~/.config/funder/config.toml
[backup]
bucket        = "..."       # required
profile       = "funder"    # default; a profile in ~/.aws/credentials
interval_days = 7           # default
```

A backup is `funder-<timestamp>.db.zst`, a zstd-compressed copy of the database, at the root of a
bucket that holds nothing else. `funder.tf` creates the bucket and an IAM user that can only
`PutObject`, and only as a conditional write that never replaces an existing backup. One-time setup:

```bash
terraform init && terraform apply
aws configure set aws_access_key_id "$(terraform output -raw funder_access_key_id)" --profile funder
aws configure set aws_secret_access_key "$(terraform output -raw funder_access_key_secret)" --profile funder
aws configure set region us-east-2 --profile funder
terraform output -raw backup_bucket   # goes in config.toml's `bucket`
```

`funder backup --status` prints the last upload and the next due date; `funder backup --force`
uploads regardless of the schedule.

To restore, quit `funder`, then under your own AWS identity (`zstd` must be installed):

```bash
aws s3 ls s3://<bucket>/
rm -f ~/.local/share/funder/funder.db-wal ~/.local/share/funder/funder.db-shm
aws s3 cp s3://<bucket>/funder-20260820T140305Z.db.zst .
zstd -d -f funder-20260820T140305Z.db.zst -o ~/.local/share/funder/funder.db
```

## No real data in the repository

This repository is public and the owner's holdings are not. Nothing committed here carries a real
amount, ticker, institution, or name, and test fixtures use invented figures. `AGENTS.md` states
the rule in full.

## License

[MIT](LICENSE)
