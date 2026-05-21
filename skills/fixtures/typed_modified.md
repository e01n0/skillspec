---
name: health-checker
input:
  service_url: string
  metrics: string[]
  timeout_ms: int (default: 5000)
  retries: int (default: 3)
output:
  report: HealthReport
description: Check the health of a service by collecting the specified metrics and producing a structured health report.
---

## Context

Check the health of a service by collecting the specified
metrics and producing a structured health report.

## Step: probe

Send a health check request to the service URL.
Collect each requested metric. Respect the timeout.
Retry up to the configured number of times on transient failures.

## Step: evaluate

Compare each collected metric against its threshold.
Determine the overall service status:
- healthy: all metrics within thresholds
- degraded: some metrics outside thresholds
- down: service unreachable or critical metrics failed

## Step: report

Assemble the HealthReport with all metrics, the
determined status, and a timestamp.
