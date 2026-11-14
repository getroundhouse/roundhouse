# Raises an AWS Health event when a quota passes 80% or 95%, such as the EC2 vCPUs RunsOn's runners use, and AWS Health
# emails each one to the account's root address, and to its operations contact if it has one, as it does any event
# about operations. A User Notifications configuration of its own would send each one twice. Notify only, since those
# quotas cap how many runners can run at once, so they're raised by hand.
resource "aws_servicequotas_auto_management" "this" {
  opt_in_level = "ACCOUNT"
  opt_in_type  = "NotifyOnly"
}

# The Region User Notifications stores the account's notifications in. AWS won't deregister an account's last hub.
resource "aws_notifications_notification_hub" "this" {
  notification_hub_region = "us-east-2"
}
