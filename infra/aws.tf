provider "aws" {
  region = "us-east-2"

  # Only this account, so an apply under another profile stops before it changes anything.
  allowed_account_ids = ["752393545121"]
}

data "aws_region" "current" {}
