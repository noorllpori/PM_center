from pmc_plugin import progress, result, run, toast


def handle(request):
    selected = request.get("selectedItems", [])
    first_name = selected[0]["name"] if selected else "nothing"

    toast(
        f"Hello from plugin. Current selection: {first_name}",
        title="Hello Plugin",
        tone="success",
    )
    progress(25)
    print("Example plugin is running...", flush=True)
    progress(100)
    result({
        "selectionCount": len(selected),
        "firstItem": first_name,
    })


if __name__ == "__main__":
    run(handle)
