# Roundhouse's AWS account, applied by hand, as README.md says.
terraform {
  # state.tf makes the bucket, so the first apply keeps its state locally.
  backend "s3" {
    bucket       = "roundhouse-terraform-state-752393545121-us-east-2-an"
    key          = "infra/terraform.tfstate"
    region       = "us-east-2"
    use_lockfile = true
  }

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "6.73.0"
    }
    sops = {
      source  = "carlpett/sops"
      version = "1.4.1"
    }
  }
}
