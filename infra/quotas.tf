# Emails when a quota passes 80% or 95%, such as the EC2 vCPUs RunsOn's runners use. Notify only, since those quotas
# cap how many runners can run at once, so they're raised by hand.
resource "aws_servicequotas_auto_management" "this" {
  opt_in_level     = "ACCOUNT"
  opt_in_type      = "NotifyOnly"
  notification_arn = aws_notifications_notification_configuration.quotas.arn
}

resource "aws_notifications_notification_configuration" "quotas" {
  name        = "service-quotas"
  description = "Service Quotas' notices, by email"

  # User Notifications takes a configuration only once the account has a hub.
  depends_on = [aws_notifications_notification_hub.this]
}

resource "aws_notifications_notification_hub" "this" {
  notification_hub_region = "us-east-2"
}

# AWS emails a code that activates it.
resource "aws_notificationscontacts_email_contact" "alerts" {
  name          = "alerts"
  email_address = local.secrets["alert_email"]
}

resource "aws_notifications_channel_association" "quotas" {
  arn                            = aws_notificationscontacts_email_contact.alerts.arn
  notification_configuration_arn = aws_notifications_notification_configuration.quotas.arn
}
