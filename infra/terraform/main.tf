# Managed infrastructure for the off-chain services. The chain needs none of this:
# if every resource here disappears, anyone can rebuild it from the contracts.
#
#   terraform init && terraform apply -var-file=testnet.tfvars

terraform {
  required_version = ">= 1.6"
  required_providers {
    neon = {
      source  = "kislerdm/neon"
      version = "~> 0.6"
    }
    vercel = {
      source  = "vercel/vercel"
      version = "~> 2.0"
    }
  }
}

variable "environment" {
  type        = string
  description = "testnet or mainnet"
}
variable "github_repo" {
  type    = string
  default = "igboze/Claw"
}
variable "api_url" {
  type = string
}
variable "contracts" {
  type = object({
    token        = string
    ve           = string
    governor     = string
    constitution = string
    agent        = string
  })
}

provider "neon" {}   # NEON_API_KEY
provider "vercel" {} # VERCEL_API_TOKEN

# App database: one project per environment; Neon branches give throwaway test DBs.
resource "neon_project" "app" {
  name      = "clawdao-${var.environment}"
  region_id = "aws-eu-central-1"
  # Daily snapshots and point-in-time recovery come with the project's history retention.
  history_retention_seconds = 604800
}

resource "neon_database" "clawdao" {
  project_id = neon_project.app.id
  branch_id  = neon_project.app.default_branch_id
  owner_name = neon_project.app.database_user
  name       = "clawdao"
}

resource "vercel_project" "web" {
  name           = "clawdao-${var.environment}"
  framework      = "nextjs"
  root_directory = "apps/web"
  git_repository = {
    type = "github"
    repo = var.github_repo
  }
}

locals {
  web_env = {
    NEXT_PUBLIC_NEAR_NETWORK          = var.environment
    NEXT_PUBLIC_API_URL               = var.api_url
    NEXT_PUBLIC_TOKEN_CONTRACT        = var.contracts.token
    NEXT_PUBLIC_VE_CONTRACT           = var.contracts.ve
    NEXT_PUBLIC_GOVERNOR_CONTRACT     = var.contracts.governor
    NEXT_PUBLIC_CONSTITUTION_CONTRACT = var.contracts.constitution
    NEXT_PUBLIC_AGENT_CONTRACT        = var.contracts.agent
  }
}

resource "vercel_project_environment_variable" "web" {
  for_each   = local.web_env
  project_id = vercel_project.web.id
  key        = each.key
  value      = each.value
  target     = ["production", "preview"]
}

output "database_host" {
  value = neon_project.app.database_host
}
