from pmc_plugin import progress, refresh, result, run, toast


def handle(request):
    command_id = request.get("commandId", "hello")
    selected = request.get("selectedItems", [])
    first_name = selected[0]["name"] if selected else "nothing"
    selected_names = [item["name"] for item in selected]

    print(f"Running command: {command_id}", flush=True)

    if command_id == "hello-inline":
        toast(
            f"Inline action triggered for: {first_name}",
            title="Hello Inline",
            tone="success",
        )
        result({
            "mode": "inline",
            "selectionCount": len(selected),
            "firstItem": first_name,
        })
        return

    if command_id == "hello-batch-report":
        toast(
            f"Preparing a report for {len(selected)} file(s)",
            title="Hello Batch",
            tone="info",
        )
        progress(35)
        print("Collecting selected files...", flush=True)
        progress(100)
        result({
            "mode": "submenu-report",
            "selectionCount": len(selected),
            "items": selected_names,
        })
        return

    if command_id == "hello-batch-refresh":
        toast(
            "Refreshing the current directory from submenu action",
            title="Hello Refresh",
            tone="success",
        )
        refresh(scope="project")
        result({
            "mode": "submenu-refresh",
            "selectionCount": len(selected),
        })
        return

    toast(
        f"Hello from plugin. Current selection: {first_name}",
        title="Hello Plugin",
        tone="success",
    )
    progress(25)
    print("Example plugin is running...", flush=True)
    progress(100)
    result({
        "mode": "section",
        "selectionCount": len(selected),
        "firstItem": first_name,
    })


if __name__ == "__main__":
    run(handle)
