# This root's state, which main.tf's backend names. Account-regional, so only this account can hold the name.
resource "aws_s3_bucket" "state" {
  bucket           = "roundhouse-terraform-state-752393545121-us-east-2-an"
  bucket_namespace = "account-regional"

  lifecycle {
    prevent_destroy = true
  }
}

resource "aws_s3_bucket_versioning" "state" {
  bucket = aws_s3_bucket.state.id

  versioning_configuration {
    status = "Enabled"
  }
}
