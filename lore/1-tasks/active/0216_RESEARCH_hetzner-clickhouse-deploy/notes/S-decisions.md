---
title: 'S: Hetzner production deployment — high-level decisions'
type: synthesis
status: mature
spawned_from: ../README.md
spawns: []
tags: [deployment, decisions, hetzner, aws]
links: []
history:
  - date: '2026-05-15'
    status: mature
    note: 'High-level decision summary.'
---

# Hetzner production deployment — decisions

## Decisions

1. **Production data store** is hosted on a dedicated Hetzner server.
   The previously planned AWS-side database is being decommissioned in
   favour of the Hetzner-hosted ClickHouse.

2. **Application API stays on AWS.** Hetzner hosts only the data plane.

3. **AWS-side topology change.** Lambda functions are moved out of the
   VPC; the long-running ingestion task is moved to a public subnet
   with a public IP. The NAT Gateway is eliminated as a result.

4. **Cross-cloud authentication** between AWS-side workloads and the
   Hetzner-hosted database is based on cryptographic identity (mutual
   TLS), not network identity (IP-based filtering).

5. **Provisioning model** is infrastructure-as-code. Hardware is ordered
   manually via the Hetzner control panel; everything else is declared
   in version-controlled configuration.
