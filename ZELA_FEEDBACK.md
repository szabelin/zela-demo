# Zela Platform Feedback

## Issues Encountered

### 1. Method Hash Discovery (UX Issue)
**Problem:** After deploying a procedure, finding the method hash to call the endpoint is not intuitive.

**Current workflow:**
1. Deploy procedure via Zela dashboard
2. Go to Procedures page
3. Find the commit hash in the Builds table
4. Manually construct: `zela.<procedure-name>#<commit-hash>`

**Suggestion:**
- Show the full callable method string prominently on the procedure detail page
- Add a "Copy method" button for easy copying
- Example display: `zela.deploy-leader-routing-RPC-v1#e409a0be04c7e9c39bc90b32d6827cf099581100`

### 2. Delete Old Procedures (Feature Request)
**Problem:** No visible way to delete old/unused procedures from the dashboard.

**Use case:**
- During development, multiple test deployments accumulate
- Old procedures clutter the Procedures list
- No cleanup mechanism visible in UI

**Suggestion:**
- Add a "Delete" button on procedure detail page
- Or add bulk delete from procedures list
- Consider soft-delete with recovery period

### 3. Real-time Dashboard Updates (Feature Request)
**Problem:** Dashboard requires manual refresh to see updated values (build status, metrics, logs).

**Current behavior:**
- Deploy a procedure → must refresh page to see build status change
- Insights/metrics don't update live
- No indication when data is stale

**Suggestion:**
- Add WebSocket connection for real-time updates
- Build status should update live (building → success/failed)
- Insights graphs should stream new data points
- Show "live" indicator when connected, "stale" when not

---

*Feedback from leader_routing development session - 15 Feb 2026*
