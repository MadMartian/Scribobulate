# Project Plan Review

## Summary

The team will {==ship the redesign by Friday==}{>>is this realistic given QA?<<}, assuming no blockers.

Metrics: {==CTR up 12%==}{>>vs which baseline?<<} and {==bounce down 3%==}{>>over what window?<<}.

## Risks

Deployment is fully automated{>>who owns the rollback runbook?<<} and low-risk.

We propose {++adding a staging gate++}, removing {--the manual approval step--}, and changing the cache from {~~30s~>5s~~}.

## Edge cases

A stray marker like {== this one never closes and must render literally.

An annotation that {==starts here

and ends across a blank line==} is out of spec and must stay literal.
