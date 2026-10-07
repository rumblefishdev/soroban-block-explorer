# Production's and testnet's API records share this module and differ only by
# workspace (task 0553). Applying one environment's tfvars in the other's
# workspace would hand production's record to testnet's state, or overwrite
# it with testnet's host — and a later destroy would take production's API
# off the internet. Workspace `testnet` holds only a testnet host, `default`
# only production's; anything else stops at plan, before a change.
resource "terraform_data" "workspace_guard" {
  lifecycle {
    precondition {
      condition     = (terraform.workspace == "testnet") == strcontains(var.api_hostname, "testnet")
      error_message = "api_hostname ${var.api_hostname} does not belong in workspace ${terraform.workspace}: a testnet host goes in workspace testnet, production's in default."
    }
  }
}
