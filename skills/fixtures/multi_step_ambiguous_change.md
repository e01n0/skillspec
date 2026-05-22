---
name: research-report
input:
  topic: string
  depth: enum("shallow", "moderate", "deep") (default: "moderate")
output:
  report: Report
description: Research the given topic and produce a structured report with multiple sections and a conclusion.
---

## Context

Research the given topic and produce a structured report
with multiple sections and a conclusion.

## Step: gather

Collect information and key facts about the topic.
Breadth of research should match the requested depth.

## Step: organise

Structure the gathered information into logical sections.
Each section should cover one aspect of the topic.

Cross-reference findings against authoritative sources before
finalising each section. Flag any contradictions found.

## Step: conclude

Write the conclusion by synthesising the organised sections.
The conclusion should answer the original research question.
