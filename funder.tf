terraform {
  # 1.10 is the floor for use_lockfile below.
  required_version = ">= 1.10"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.37"
    }
  }

  backend "s3" {
    bucket = "jluszcz-tf-state"
    key    = "funder"
    region = "us-east-2"

    # S3-native locking; no DynamoDB table required.
    use_lockfile = true
  }
}

variable "aws_region" {
  type    = string
  default = "us-east-2"
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      ManagedBy = "terraform"
      Repo      = "Funder"
    }
  }
}

data "aws_caller_identity" "current" {}

/**************************************
* Backup Bucket
**************************************/

# Composed from the caller's own account and region rather than chosen, which is
# what lets it be written in a public repository: a chosen name would say where
# the owner's holdings are backed up, while this one is legible only to whoever
# already holds the profile.
resource "aws_s3_bucket" "backup" {
  bucket           = format("funder-%s-%s-an", data.aws_caller_identity.current.account_id, var.aws_region)
  bucket_namespace = "account-regional"
}

resource "aws_s3_bucket_public_access_block" "backup" {
  bucket = aws_s3_bucket.backup.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# The AWS-managed aws/s3 key, stated rather than left to the account default,
# because the IAM policy below leans on it: that key's own policy grants
# same-account principals use of it via kms:ViaService, so the backup identity
# needs no kms:GenerateDataKey grant.
resource "aws_s3_bucket_server_side_encryption_configuration" "backup" {
  bucket = aws_s3_bucket.backup.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "aws:kms"
    }
    bucket_key_enabled = true

    # Declared to match what S3 already enforces; left out, the provider
    # plans it away on every run.
    blocked_encryption_types = ["SSE-C"]
  }
}

# A whole-bucket resource, which is why the bucket is this application's own.
resource "aws_s3_bucket_lifecycle_configuration" "backup" {
  bucket = aws_s3_bucket.backup.id

  rule {
    id     = "expire"
    status = "Enabled"

    filter {}

    expiration {
      days = 365
    }
  }

  # A month-old backup is a restore point, and Infrequent Access prices that:
  # cheaper to keep, dearer to read, and nothing reads one on a schedule. 30
  # days is IA's minimum billable duration. IA also bills a 128 KB floor per
  # object and S3 declines to transition anything smaller, so on a small
  # database this rule does nothing, which is the right way round.
  rule {
    id     = "transition-ia"
    status = "Enabled"

    filter {}

    transition {
      days          = 30
      storage_class = "STANDARD_IA"
    }
  }

  rule {
    id     = "expire-noncurrent"
    status = "Enabled"

    filter {}

    noncurrent_version_expiration {
      noncurrent_days = 30
    }
  }

  rule {
    id     = "abort-mpu"
    status = "Enabled"

    abort_incomplete_multipart_upload {
      days_after_initiation = 7
    }
  }
}

/**************************************
* Backup IAM User
**************************************/

resource "aws_iam_user" "funder" {
  name = "funder"
}

resource "aws_iam_access_key" "funder" {
  user = aws_iam_user.funder.name
}

data "aws_iam_policy_document" "funder" {
  # PutObject and nothing else. The key is long-lived and unattended, so the
  # policy is what bounds it: a stolen key can add objects, but cannot read,
  # overwrite, or delete a backup, or list the bucket. Restores run under the owner's own
  # identity.
  #
  # The whole bucket rather than a prefix: it holds nothing but backups, and a
  # prefix here would have to match
  # `jluszcz_finance_utils::backup::Spec::key_for` with nothing tying the two
  # together but `AccessDenied`.
  statement {
    actions   = ["s3:PutObject"]
    resources = ["${aws_s3_bucket.backup.arn}/*"]

    # Only with `If-None-Match: *`, so a put can create a backup but never
    # replace one. Without it the bucket has no versioning to fall back on,
    # and an overwrite would destroy a backup as surely as a delete.
    condition {
      test     = "Null"
      variable = "s3:if-none-match"
      values   = ["false"]
    }
  }
}

resource "aws_iam_user_policy" "funder" {
  name   = "funder"
  user   = aws_iam_user.funder.name
  policy = data.aws_iam_policy_document.funder.json
}

# What goes in config.toml's `bucket`.
output "backup_bucket" {
  value = aws_s3_bucket.backup.bucket
}

output "funder_access_key_id" {
  value = aws_iam_access_key.funder.id
}

# Otherwise reachable only by reading raw state.
output "funder_access_key_secret" {
  value     = aws_iam_access_key.funder.secret
  sensitive = true
}
