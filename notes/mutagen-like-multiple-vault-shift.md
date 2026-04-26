# Mutagen-like multiple vault shift

The v0 shipping gate (step 5) happened way quicker than I was expecting. So, I want to take a step back and reevaluate some of the priorities and plans before the implementation gets too far along to course correct on some core ideas. The goal is still similar — keep the scope limited *but usable* — but this particular issue I think deserves more consideration than it was given previously.

The real push for this now is the realization that the UX for maintaining multiple vaults would be far from what I'd want it to be. I realized this pattern of "one instance per thing" is a configuration and state nightmare.

One daemon per vault requires maintaining multiple sets of configuration (either multiple sets of configuration files or multiple sets of systemd units with a lot of command line flags to specify which directory "this instance" is supposed to pay attention to) and it ormakes consuming more complicated (right now, `hmn` or an MCP consumer can connect to the same host/port no matter what; if multiple instances exist, this now requires `hmn` or an MCP consumer to be host/port aware, somehow).

One daemon per vault requires state management at a different level as well. If the daemon only ever has one path it writes to for state-related content, and there are more than one `hmnd` running at the same time, do the state files need to be lock-safe? Do they need to be vault-specific? Think like `mysqld` where each database has it's own *directory* where it stores database information. If there is a pidfile or something, we can no longer assume we have just `/var/run/hmnd.pid` since we might need more than one PID tracked.

This exploration is specifically about finding out what impact "multiple vaults" would have on the current design. It does *not* necessarily mean that we have to do it now (before step 6 of the current roadmap). However, I *do* want to set the direction and start planning this functionality correctly. I tried running my new `spec-generator` skill and it blocked it outright because it goes against the existing LDS canon as you've just confirmed.

---

In the discussion around this so far, a few things have surfaced that make me glad we started talking about it more. There is one specific flavor I'd like to explore that will potentially have a big impact on a lot of things.

One tool I mentioned early on for a different angle was Mutagen. I mentioned that Mutagen has an `ignore_vcs_files` option that can be configured on a per-sync basis.

Thinking on it now, I don't know why I didn't notice earlier...

Some aspects the Mutagen model work exactly like I'd like the vault to work. I *think* that because I'm explicitly trying to avoid anything **sync**-related in this project might be why I blocked my mind from realizing this.

The aspects I like about how Mutagen manages it's two major features — synchronization and forward — is that it's all very well structured and organized in the same way that `docker` and `docker compose` are. I'm going to focus on `sync` because I know that best. Please keep in mind, this very important detail: **at no point is any of this discussion going to imply that Hypomnema will do anything related to sync**.

```bash
# Create a synchronization session named "web-app-code" between the local path
# ~/project and an SSH-accessible endpoint.
mutagen sync create --name=web-app-code ~/project user@example.org:~/project
```

```bash
# List all sync sessions
mutagen sync list
```

```
Name: web-app-code
Identifier: sync_rJA9OdPDEtIVcqwhOMlBw2BvMgpctZXUsEr4Jl3kUd7
Labels: None
Alpha:
    URL: /home/user/project
    Connection state: Connected
Beta:
    URL: user@example.org:~/project
    Connection state: Connected
Status: Watching for changes
```

```bash
mutagen sync monitor web-app-code
```

```
Name: web-app-code
Identifier: sync_rJA9OdPDEtIVcqwhOMlBw2BvMgpctZXUsEr4Jl3kUd7
Labels: None
Alpha: /home/user/project
Beta: user@example.org:~/project
Status: Staging files on beta: 75% (8942/11782)
```

```bash
# Pause the synchronization session named "web-app-code".
mutagen sync pause web-app-code
```

```bash
# Pause the synchronization session named "web-app-code".
mutagen sync resume web-app-code
```

```bash
# Reset the synchronization session named "web-app-code".
mutagen sync reset web-app-code
```

```bash
# Terminate the synchronization session named "web-app-code".
mutagen sync terminate web-app-code
```

One difference between Mutagen and Hypomnema is the CLI vs Daemon distinction. I think that Mutagen has a daemon, and `mutagen` itself might be the daemon? It also has a `mutagen-agent` which *might* act as the daemon both locally and on the remote target. I don't want to get into the weeds on this one, though. I just wanted to clarify that there is a distinction worth noting:

- Mutagen's user-facing CLI tool (`mutagen`) is primarily for interacting with the configuration of the Mutagen daemon (`mutagen-agent`?)
- Hypomnema's user-facing CLI tool (`hmn`) primarily for interacting with an already-configured and running instance of the daemon (`hmnd`)
- Even though Mutagen's `sync` command belongs to `mutagen` (its user-facing CLI tool), I believe that the equivalent Hypomnema's command may belong to `hmnd` (its daemon) or something specifically for interacting with the daemon's state

Here is how I see the Mutagen-style commands translating to `hmnd` / `hmn` / `hmndctl`. In all cases, these commands expect that an `hmnd` instance is already running.

> [!NOTE] For the first few examples, I'm showing that by not specifying a name, it will fallback to using "default". Later examples in this set will only show with the example vault named "personal"; this does NOT mean that those commands will not support the nameless method.

```
# Create a vault named "default"
hmnd vault create ~/personal-vault

# Create a vault named "personal"
hmnd vault create --name="personal" ~/personal-vault

# List all vaults and show their status
hmnd vault list

# Shows the status of a vault named "personal"
hmnd vault status personal

# Pause the vault named "personal"
hmnd vault pause personal

# Resume the vault named "personal"
hmnd vault resume personal

# Reset the vault named "personal"
hmnd vault reset personal

# Terminate the vault named "personal"
hmnd vault terminate personal
```

Another thing I'd consider here is that we could do something like `hmndctl` for this command instead of using `hmnd` itself. It might seem awkward or weird to call the daemon binary and have it interact with the state of an already-running daemon and then exit right away.

I do not think I like the idea of having something like `hmn vault` work, even though that might feel more natural?  But I'm willing to consider this. It fits better with Mutagen's model, but it means that the `hmn` command is now "interacting with a running `hmnd`" and also "configuring `hmnd`". I'd like push back on this because I don't really know how strongly I feel about this.

If the main `hmn` interface becomes something where you can do `hmn vault ...`, that might mean the MCP can also potentially use the same functionality, and now all aspects of the vault management can be controlled via AI agents. So, maybe `hmn vault` *is* the path.

See? I'm of many minds on this. :) Throughout this document, the examples I'm showing follow the pattern of `[HNM] vault ...`, where in early I was using `hnmd vault ...` but later mostly using `hmn vault ...` While `hmndctl` is proposed, the only examples showing it are listed directly below:

```bash
hmndctl vault create --name="personal" ~/personal-vault
hmndctl vault list
hmndctl vault status personal
hmndctl vault pause personal
hmndctl vault resume personal
hmndctl vault reset personal
hmndctl vault terminate personal
```

---

I believe Mutagen is very specific about the "name" being tied to the resource. So, you could have a name "foo" for both a sync and a forward and that's fine. We only have vaults (currently), so that distinction probably doesn't matter.

Mutagen also abstracts the concept of "a sync" and "a forward" to "a session". A session has a unique identifier (like `sync_rJA9OdPDEtIVcqwhOMlBw2BvMgpctZXUsEr4Jl3kUd7`). If a name is specified, it *also* has a name. So all of the commands that take "a sync" or "a forward" can actually take the sync name, forward name, or the session ID. It's at *this level* that it matters what you are passing it to. For example, if you can have both a sync and a forward named "foo", you can pass "foo" to either. However, if you try to pass "bar" to either, it will only work if there is a "bar" for the one you passed to (`mutagen sync status bar` if you had only created a forward named `bar` should be an error). Likewise, you can't pass the session ID of a sync to a forward without it being an error.

Mutagen also uses "labels" as an identifier but I think we don't need to worry about that at all for Hypomnema.

I don't *think* we need to abstract to "a session" in the sense that multiple things (not just vaults) might be handled in the future. But I do like the idea of a generated unique ID in addition to the vault name to be the "primary key" as a surrogate key.

See more information about [Mutagen Names, Labels, and Identifiers](https://mutagen.io/documentation/introduction/names-labels-identifiers/).

---

More details on the specifics of the actions.

**Status** — The status can include details like index size/number of items, whether the index is up-to-date, the number of files watched, etc. It can also include whether it is paused.

**Pause** — This might be too generic, but I'd guess this *could* mean the indexer, the watcher, etc. I don't think this would mean, "remove from the list of available vaults accessible via the MCP or CLI"; it's more an operational thing. It also might not be needed. If we end up wanting to be more specific and have things we can actually control independently (`hmn vault pause personal --watcher)

**Resume** — The other side of **Pause** with the same level of questions on whether we need this and whether we need to expose things on a more granular level or not.

**Reset** — This would be to clear any error states that we might have that might impact whether `hmnd` has stopped doing something on the vault *separately from if we explicitly paused*. This could also be a way to clear things out from time to time where it would basically say, "close all the things related to this vault — the indexer, the watcher, etc. — and start again from scratch like the daemon just started".

**Rename** — This wasn't on the list above because it doesn't have a pair with the `mutagen` commands. But, the vault name is probably more important than the "name" of a sync session, and might be nice to be able to rename the vault rather than terminating it and create a new vault with the right name. For Mutagen, terminate and create are quick, since it's mostly metadata that can be gathered easily by file hashes and what not. There is a real cost to building the indexes for a Hypomnema vault. It would be better to just update the name than have to tear it down just to bring it up again.

```bash
# Rename the vault named "default" to "my-personal-vault"
hmn vault rename --name my-personal-vault

# Rename the vault named "personal" to "my-personal-vault"
hmn vault rename personal --name my-personal-vault
```

**Rescan** — This wasn't on the list above because it doesn't have a pair with the `mutagen` commands. But, this is where I think we could move what was originally set up as `hmnd scan`. So:

```bash
# Rescan the vault named "default"
hmn vault rescan

# Rescan the vault named "personal"
hmn vault rescan personal
```

**Terminate** — This would remove the vault from `hmnd`'s state entirely. It would NOT do anything with the files in directory the vault was configured against. It should be safe and cheap. Someone should be able to call `hmn vault terminate personal` and follow it up immediately with `hmn vault create --name=personal ~/personal-vault` without any problems. When I say "cheap", I mean, it should not destroy any data in the vault itself (`~/personal-vault`) and it should be relatively fast ("stop watching, stop indexing, remove internal references to the vault").

---

Where I see this having a big impact on the current overall design is that everything related to the vault's definition is currently in the main configuration file. At minimum, supporting "multiple vaults" was going to require a config change. It made me realize that the vault configuration itself could be considered *state* instead of *configuration*. Which port does `hmnd` listen on? Configuration. Which directories are currently watched? State!

This is why I wanted to jump on this here, now, to at least figure out what we can do and whether it makes sense to build this in to some level — beyond the already implemented hedge on adding `vault` to the structures that landed in step 5.

Moving vault from config to something managed in state (might still end up in a file that looks like a configuration file?) versus something that is explicitly configured by hand by the user, is a big shift.

---

The concept of a "default" might need to go away. It's there for UX reasons where someone doesn't have to actually give a name to anything and things will jut work. If it adds too much complexity to support optional/default-if-not-specified for all these actions, I'm fine just deciding people have to name things.

If the concept of a "default" stays, the implication is NOT that `hmnd` will always create a default vault if you don't create one explicitly. So, vault commands that assume default (except for `hmn vault create`), on a fresh instance of `hmnd` running, would fail:

```bash
# The following commands will all fail since "default" does not exist
hmnd vault status
hmnd vault pause
hmnd vault resume
hmnd vault reset
hmnd vault terminate

# The following command will create a vault named "default"
hmnd vault create ~/personal-vault

# The following commands will all work since "default" exists
hmnd vault status
hmnd vault pause
hmnd vault resume
hmnd vault reset
hmnd vault terminate
```

The "default" vault is nothing special beyond the string "default" being the name. In fact, we should be able to make this configurable in the main configuration with something like "default_vault_name: default" so someone could change that if they want to.

---

Ideas related to this that might already have open questions on but we could discuss more now if it makes sense to do so:

- Can someone search across multiple vaults?
- Can someone specify *which* vaults to include in their search?
