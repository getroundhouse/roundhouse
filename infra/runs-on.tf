# RunsOn's Flex control plane, which launches an EC2 runner for each job whose label names one of
# .github/runs-on.yml's runners.
module "runs_on" {
  source  = "runs-on/runs-on/aws//modules/flex"
  version = "v3.6.1"

  github_organization = "getroundhouse"
  license_key         = local.secrets["runs_on_license_key"]
  email               = local.secrets["alert_email"]

  # The module tags everything with this anyway, and its default, stack, would add a duplicate.
  cost_allocation_tag = "runs-on-stack-name"

  vpc_id            = aws_vpc.runs_on.id
  public_subnet_ids = [for subnet in aws_subnet.runs_on : subnet.id]

  # Cheaper while idle. A reclaimed worker's webhooks wait in SQS for the next.
  app_capacity_provider = "fargate_spot"
  # The account doesn't encrypt new volumes by default.
  ebs_encryption_key_id = "alias/aws/ebs"

  # Past CI's 60-minute job timeout, since a runner boots before its job starts.
  runner_max_runtime = 75
  # The default extends a `.github-private` repository's config, which the org doesn't have.
  runner_config_auto_extends_from = "."

  # The repository is public, so a job's cache credentials reach only its own branch, and read main's.
  enable_cache_isolation = true
  # No job uses sticky disks, so the runners' role needs no EBS access.
  enable_stickydisk_isolation = true
  # main saves the tests' cache only when a dependency changes, and a restore doesn't extend an entry.
  cache_expiration_days = 30
  # A fixed name, runs-on-cache-<account>-<region>-an, rather than a random suffix.
  cache_bucket_namespace = "account-regional"
  # Only caches and logs.
  force_destroy_buckets = true

  # The setup pages, which create the GitHub App. Off once it exists.
  enable_admin_routes = false
}

# Public subnets rather than a NAT gateway, which bills by the hour and the gigabyte. Every zone widens Spot capacity.
# Each zone's number places its subnet, so removing one leaves the others alone.
locals {
  runs_on_zones = {
    "us-east-2a" = 0
    "us-east-2b" = 1
    "us-east-2c" = 2
  }
}

resource "aws_vpc" "runs_on" {
  cidr_block           = "10.200.0.0/16"
  enable_dns_hostnames = true

  tags = {
    Name = "runs-on"
  }
}

resource "aws_subnet" "runs_on" {
  for_each = local.runs_on_zones

  vpc_id            = aws_vpc.runs_on.id
  availability_zone = each.key
  cidr_block        = cidrsubnet(aws_vpc.runs_on.cidr_block, 4, each.value)

  tags = {
    Name = "runs-on-${each.key}"
  }
}

resource "aws_internet_gateway" "runs_on" {
  vpc_id = aws_vpc.runs_on.id

  tags = {
    Name = "runs-on"
  }
}

resource "aws_route_table" "runs_on" {
  vpc_id = aws_vpc.runs_on.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.runs_on.id
  }

  tags = {
    Name = "runs-on"
  }
}

resource "aws_route_table_association" "runs_on" {
  for_each = aws_subnet.runs_on

  subnet_id      = each.value.id
  route_table_id = aws_route_table.runs_on.id
}

# Keeps S3 traffic, the cache's included, inside the VPC. The endpoint is free, and so is the transfer either way.
resource "aws_vpc_endpoint" "runs_on_s3" {
  vpc_id            = aws_vpc.runs_on.id
  service_name      = "com.amazonaws.${data.aws_region.current.region}.s3"
  vpc_endpoint_type = "Gateway"
  route_table_ids   = [aws_route_table.runs_on.id]

  tags = {
    Name = "runs-on-s3"
  }
}

# The setup page's link, while enable_admin_routes is on.
output "runs_on_getting_started" {
  value = module.runs_on.stack.getting_started
}
