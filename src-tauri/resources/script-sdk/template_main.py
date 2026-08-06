from nexora_sdk import log, progress, raise_if_cancelled, read_request, set_state, write_error, write_result


def main():
    request = read_request()
    if request.command != "run":
        write_error(f"unsupported command: {request.command}")
        return

    log(f"run={request.context.run_id} project={request.context.project_path}")
    raise_if_cancelled(request.context)
    progress(0.5, "processing")
    set_state("lastRunId", request.context.run_id)
    write_result({
        "message": "Nexora script component is working",
        "input": request.payload,
        "projectPath": request.context.project_path,
    })


if __name__ == "__main__":
    main()
