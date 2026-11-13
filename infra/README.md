# Infrastructure

Terraform for Roundhouse's AWS account, in `us-east-2`. It holds [RunsOn](https://runs-on.com)'s stack, which launches an EC2 instance for each CI job that names one of its runners, the bucket this root's state is kept in, and the key its secrets are encrypted with. Nothing applies it but a person, by hand.

| File | What it holds |
|---|---|
| `main.tf` | The backend, and the providers' versions |
| `aws.tf` | The AWS provider, which refuses any other account |
| `runs-on.tf` | RunsOn's stack, and the VPC its runners start in |
| `state.tf` | The bucket this root's state is kept in |
| `quotas.tf` | Service Quotas' email when a quota nears its limit |
| `secrets.tf` | The KMS key that encrypts `secrets.enc.yaml`, and the values Terraform reads from it |
| `secrets.enc.yaml` | RunsOn's license key, and the address alerts go to, encrypted with SOPS |
| `.sops.yaml` | The key SOPS encrypts a new file with |
| `../.github/runs-on.yml` | The runners a job's label can name |

This root takes no variables: each value is in the file it configures.

Working on it needs Terraform 1.10 or later, SOPS 3.9 or later, and the AWS CLI.

## Runners

A job names a runner in its label, as in `runs-on: runs-on=${{ github.run_id }}/runner=arm64-16`, and the label can override the runner's settings, as in `/spot=false`. A job that caches runs `runs-on/action` first, so `actions/cache` reaches the stack's bucket rather than GitHub's. A run can save only to its own branch's cache, and reads `main`'s too, as on GitHub.

RunsOn reads `.github/runs-on.yml` only from `main`, since the repository is public, so a change to a runner reaches every job that names it once it merges, without a PR having run on it. To change a runner jobs use, add one under a new name, merge it, and move the jobs to it in a PR of its own, whose CI runs on it.

While the stack can't start a runner, such as while the account is at its EC2 quota, a job waits for one, for up to the 24 hours GitHub lets a job wait for a self-hosted runner. To run CI without the stack, point the job's `runs-on` back at one of GitHub's runners, such as `ubuntu-26.04-arm`.

## Planning and applying

With `AWS_PROFILE` naming a profile for the account, and signed in, plan from this directory:

```sh
terraform init
terraform plan
```

An apply changes the account, so it's for the owner, or for someone the owner asked. `terraform apply` shows the plan again before it changes anything. A plan saved with `-out` holds the secrets in plain text, so keep it out of the repository.

After a provider or module update, record both platforms' hashes, so a Mac and Linux agree on the lockfile:

```sh
terraform providers lock -platform=darwin_arm64 -platform=linux_amd64
```

## Setting it up

A new account can run only 5 vCPUs of Spot instances and 5 of on-demand ones, which is less than one `arm64-16` runner. Quotas are raised by hand, and `quotas.tf` emails when one passes 80% or 95%. Ask for room for 8 runners on Spot and 4 on-demand, which covers Spot when it has no capacity, and the jobs RunsOn reruns when AWS reclaims an instance:

```sh
aws service-quotas request-service-quota-increase --region us-east-2 --service-code ec2 --quota-code L-34B43A08 --desired-value 128
aws service-quotas request-service-quota-increase --region us-east-2 --service-code ec2 --quota-code L-1216C47A --desired-value 64
```

The state bucket and the KMS key are in this root, so the first apply can't keep its state in the bucket, or read secrets the key hasn't encrypted yet. It makes only those two, with a local backend, in an uncommitted `backend_override.tf`:

```hcl
terraform {
  backend "local" {}
}
```

```sh
terraform init
terraform apply -target=aws_s3_bucket_versioning.state -target=aws_kms_alias.secrets
```

Then delete the override, move the state into the bucket, which `init` asks to do, and delete the local copies:

```sh
rm backend_override.tf
terraform init -migrate-state
rm terraform.tfstate terraform.tfstate.backup
```

Put the secrets in with SOPS, from this directory, which opens an editor and encrypts what's saved. The license key comes from [RunsOn's site](https://runs-on.com/pricing/). Commit the file, since only the key can read it:

```sh
sops edit secrets.enc.yaml
```

```yaml
runs_on_license_key: ...
alert_email: ...
```

Set `enable_admin_routes` to `true` in `runs-on.tf`, which turns on RunsOn's setup page, and apply the rest:

```sh
terraform apply
```

The apply prints `runs_on_getting_started`, which links to the setup page. It creates RunsOn's GitHub App in the `getroundhouse` org, to install there. Confirm the two emails AWS sends to `alert_email`, one for RunsOn's alerts and one with a code for Service Quotas' notices, so both reach it. Then set `enable_admin_routes` back to `false`, and apply again. To make the GitHub App again, turn it on for as long as that takes.

In a member account of an AWS Organization, AWS can refuse RunsOn's daily budget until the account that pays the bill has turned budgets on. The apply then fails on the budget alone, after making everything else, and a later apply makes it. RunsOn's daily budget and cost reports read costs by the `runs-on-stack-name` tag. Turn it on under cost allocation tags in Billing, in the account that pays the bill, which for a member of an AWS Organization is its management account. Billing lists a key up to a day after the stack first uses it.
