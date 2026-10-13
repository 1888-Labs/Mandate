# Inspiration and related work

[← Back to the README](../README.md)

Mandate's main inspiration is **Google Zanzibar**, Google's global authorization system. A second Google paper, on **distributed grammars**, is closely related: it deals directly with delegation and authorization in distributed systems.

## Zanzibar: Google's Consistent, Global Authorization System

> Ruoming Pang, Ramón Cáceres, Mike Burrows, Zhifeng Chen, Pratik Dave, Nathan Germer, Alexander Golynski, Kevin Graney, Nina Kang, Lea Kissner, Jeffrey L. Korn, Abhishek Parmar, Christina D. Richards, Mengzhi Wang. *USENIX Annual Technical Conference (ATC)*, 2019.
> [Google Research](https://research.google/pubs/zanzibar-googles-consistent-global-authorization-system/) · [USENIX](https://www.usenix.org/conference/atc19/presentation/pang)

Zanzibar stores and evaluates access-control lists for hundreds of Google services, including Drive, YouTube, Calendar, Cloud and Photos, through one shared data model and one authorization service.

What Mandate takes from Zanzibar:

- **One shared source of authorization truth.** Every service asks the same system "is this allowed?" instead of keeping its own permissions. In Mandate that shared state lives on Arkiv.
- **A generic model.** Zanzibar isn't specific to any one product. Mandate's `action` and `resource` are plain strings for the same reason: payments, GitHub, databases and MCP tools all fit the same model.
- **Answering "who can do what, and why".** Decisions are queries over shared state, and Mandate makes the reasoning explicit in every ALLOW and DENY.

Where Mandate differs:

- **Delegated capabilities, not access-control lists.** Zanzibar stores relations ("user U is an editor of document D"). Mandate stores signed grants that can be narrowed and passed down a chain of agents.
- **No trusted operator.** Zanzibar is run by Google and trusted by the services that call it. Mandate's records are signed by their issuers and re-verified by the engine, so the storage layer can't invent or alter authority.
- **Weaker consistency.** Zanzibar gives strong consistency guarantees, including protection against the "new enemy" problem, where a check uses stale permissions after a revocation. Mandate v0.1 checks against the latest state it reads from Arkiv and makes no such guarantee. Mandate is *inspired by* Zanzibar; it is not a Zanzibar implementation.

## Distributed Authorization with Distributed Grammars

> Martín Abadi, Mike Burrows, Himabindu Pucha, Adam Sadovsky, Asim Shankar, Ankur Taly. In *Programming Languages with Applications to Biology and Security*, Springer, 2015, pp. 10–26. [doi:10.1007/978-3-319-25527-9_3](https://doi.org/10.1007/978-3-319-25527-9_3)
> [Google Research](https://research.google/pubs/distributed-authorization-with-distributed-grammars/)

This paper designs authorization for distributed systems by treating groups as formal languages. Its design supports **delegation**, and it considers group definitions that may not all be available, or may contain cycles.

What Mandate takes from it:

- **Authority that crosses trust boundaries.** Authorization is decided by following delegations between independent parties, not by looking up a single central list. Mandate's check walks the chain Alice → FinanceAgent → InvoiceAgent and verifies every link.
- **Planning for incomplete or hostile state.** Mandate treats a missing parent mandate, an over-long chain or an unverifiable link as an explicit denial (`INVALID_DELEGATION`), never as an error or a silent ALLOW.

Mandate v0.1 does not implement the paper's group grammars or negative clauses. Its delegation model is deliberately simpler: one linear chain per mandate, where each step may only narrow authority.
