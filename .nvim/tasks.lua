return {
    {
        name = "Build debug",
        description = "cargo debug build",
        cmd = {
            "cargo",
            "build",
        }
    },
    {
        name = "Build release",
        description = "cargo release build",
        cmd = {
            "cargo",
            "build",
            "--release"
        }
    },
}
