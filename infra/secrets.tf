# Encrypts secrets.enc.yaml, which this public repository can't hold in the clear. Anyone signed in to the account can
# read it.
resource "aws_kms_key" "secrets" {
  description = "Encrypts infra/secrets.enc.yaml"

  # secrets.enc.yaml can't be read without it.
  lifecycle {
    prevent_destroy = true
  }
}

resource "aws_kms_alias" "secrets" {
  name          = "alias/roundhouse-secrets"
  target_key_id = aws_kms_key.secrets.key_id
}

data "sops_file" "secrets" {
  source_file = "${path.module}/secrets.enc.yaml"
}

locals {
  secrets = data.sops_file.secrets.data
}
