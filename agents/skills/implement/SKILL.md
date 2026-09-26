---
name: implement
description: "Implement a piece of work based on a spec or set of tickets."
disable-model-invocation: true
---

Implement the work described by the user in the spec or tickets.

Before changing anything, record the current commit as the **fixed point** for the review at the end.

## Test-first, at pre-agreed seams

Work test-first where possible, as a red → green loop. A **seam** is the public boundary you test at: the interface where you observe behaviour without reaching inside. Test only at the seams the spec or tickets agreed (their testing decisions); when they name none, use the highest existing public interface that covers the behaviour.

- **Red before green.** Write one failing test, then only enough code to pass it. One seam, one test, one minimal implementation per cycle; each test a **tracer bullet** that responds to what the last cycle taught you.
- **Test behaviour, not implementation.** A good test reads like a specification ("user can checkout with valid cart") and survives a refactor that keeps behaviour. Verify through the interface itself, not a side channel such as querying the database directly.
- **Mock only at system boundaries**: external APIs, time, randomness, sometimes the database or file system. Use the real modules you control.
- **Expected values come from an independent source of truth**: a known-good literal, a worked example, the spec. An assertion that recomputes the value the way the code does passes by construction.

Run typechecking regularly, single test files regularly, and the full test suite once at the end.

## Review

Once done, review every change since the fixed point, committed or not, along two separate axes. Run each in its own sub-agent, in parallel, so neither pollutes the other's context; give each the change and the brief below.

- **Spec**: pass the spec or tickets. Brief: "Report (a) requirements that are missing or partial, (b) behaviour in the diff that wasn't asked for (scope creep), (c) requirements that look implemented but wrongly. Quote the spec line for each finding. Under 400 words."
- **Standards**: pass the repo's documented coding standards plus the smell baseline below. Brief: "Report every place the diff violates a documented standard, citing the file and rule, and any baseline smell, naming it and quoting the hunk. Documented breaches can be hard violations; baseline smells are always judgement calls, and a documented standard overrides the baseline. Skip anything tooling enforces. Under 400 words."

Smell baseline (Fowler, _Refactoring_ ch.3), each _what it is_ → _fix_:

- **Mysterious Name**: a name that doesn't reveal what it does or holds → rename it.
- **Duplicated Code**: the same logic shape in more than one hunk or file → extract the shared shape.
- **Feature Envy**: a method reaching into another object's data more than its own → move it onto that data.
- **Data Clumps**: the same fields or params travelling together → bundle them into one type.
- **Primitive Obsession**: a primitive standing in for a domain concept → give the concept its own type.
- **Repeated Switches**: the same switch or if-cascade on the same type recurring → polymorphism, or one shared map.
- **Shotgun Surgery**: one logical change forcing scattered edits → gather what changes together.
- **Divergent Change**: one module edited for several unrelated reasons → split it.
- **Speculative Generality**: abstraction or hooks for needs the spec doesn't have → inline it back.
- **Message Chains**: long `a.b().c().d()` navigation → hide the walk behind one method.
- **Middle Man**: a unit that mostly delegates onward → call the real target directly.
- **Refused Bequest**: an implementer ignoring most of what it inherits → use composition.

Fix the findings you agree with and re-run the affected tests. Report the rest under `## Standards` and `## Spec` headings, keeping the axes separate.

Commit your work to the current branch.
