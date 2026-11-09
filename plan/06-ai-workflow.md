# 06 · Iterating on this plan with AI

**Work one task card at a time, on its own branch, with its acceptance checks as the definition of
done.** The plan is split so that one AI session needs only one phase card, the design sections it
points to, and the guide sections it names. The CI gates from P0 catch most mistakes before you
review anything.

## Loop

1. **Pick** the next `todo` task in [STATUS.md](STATUS.md) whose dependencies are done.
2. **Start a session** with the prompt below, on a branch `p<phase>-<task>-<slug>`.
3. **The AI implements**, runs the task's "Done when" checks and the global gates, and updates
   `STATUS.md`. If a design choice came up, it writes an ADR.
4. **You review** the PR with the checklist below. Merge, or comment and iterate.
5. **Change the plan** when reality disagrees: edit the card or the design doc in the same PR, and
   record why in an ADR. The plan is meant to change.

## Session prompt template

```
Implement task P<k>.<n> from plan/phases/P<kk>-<name>.md.

Before coding, read:
- plan/06-ai-workflow.md (rules) and CLAUDE.md
- the task card, and the plan sections it lists under "Read first"
- the parameter-guide sections it names (Claude Doc linked in plan/reference/)

Then:
1. Restate the task's "Done when" checks as a checklist.
2. If the task needs a modelling choice not settled in plan/03-model-design.md or an ADR,
   stop and propose options in an ADR draft instead of choosing silently.
3. Implement with tests first for each check.
4. Run: cargo fmt/clippy/test, pytest, determinism test, benchmark; report numbers.
5. Update plan/STATUS.md (status, PR link, notes) and the data dictionary if outputs changed.
Do not start the next task.
```

For **review-gate tasks** (marked "approved by the user"), the session ends with an ADR draft and no
code.

## Rules (copied into `CLAUDE.md` in P0.8)

1. **No magic numbers.** Every behavioural constant comes from the register, with grade, role,
   stage and source. A new parameter means a new register entry first.
2. **No capacity or flow multipliers, ever.** Road effects go only through width, speed caps, gap
   supply or incentives (guide §6). Published capacity losses are validation targets.
3. **Never tune toward a validation target directly.** Targets in `validation/targets/` are checks.
   If a target is missed, report the gap; don't add a fudge factor.
4. **Respect stage freezing.** Do not edit frozen `calibration/stage_*.yaml` values or `Fix`
   parameters to make a later test pass.
5. **Determinism is sacred.** No global RNG, no `HashMap` iteration order in outputs, no unordered
   parallel reductions. The determinism test must pass.
6. **Performance is a feature.** No more than 5% regression on the reference benchmark without an
   ADR. Hot-loop code stays allocation-free.
7. **Every mechanism ships with micro-scenario tests and an output.** If it can't be observed, it
   can't be validated.
8. **Baseline is not violation.** Filtering, staggered following and moving between notional lanes
   are physics, never counted as violations (guide §4.4).
9. **Explain decisions.** Every stochastic decision logs its features and logit terms.
10. **Schema changes regenerate** the Python and TS types and the data dictionary in the same PR.
11. **Licence hygiene.** The repo is MIT. Don't copy code from GPL projects; cite papers for
    equations.
12. **Ask, don't guess, on modelling.** Ambiguity in the guide → an ADR draft with options, not a
    silent choice.

## Review checklist (for you)

- [ ] The "Done when" checks are shown passing, with numbers.
- [ ] New parameters have sensible grade and role, and the source matches the guide.
- [ ] No capacity multipliers and no tuning toward targets (rules 2–3).
- [ ] Determinism and benchmark results are in the PR description.
- [ ] Any modelling choice is in an ADR you agree with.
- [ ] STATUS.md is updated.

## Tips

- **Keep sessions small.** If a task is getting long, ask the AI to split it into sub-tasks in the
  card before going on.
- **Use the viewer for behaviour tasks.** Once P4 exists, ask for a short screen recording or GIF in
  each behaviour PR. Odd lateral behaviour is obvious on screen and invisible in numbers.
- **Give the guide, not a paraphrase.** When a task cites the guide, the session should read the
  guide section itself.
- **Re-plan at phase boundaries.** At the end of each phase, ask for a short retrospective: what
  changed, and what the next phase card should say differently.
