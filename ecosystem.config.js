module.exports = {
    apps: [{
        name: "ai-uploader",
        script: "./target/release/ai-uploader",
        instances: 1,
        exec_mode: "fork",
        env: {
            NODE_ENV: "production",
            AI_UPLOADER_PORT: 6001,
            LOG_PATH_API0: "/var/log/api0.log",
            RUST_LOG: "debug"
        },
        error_file: "./logs/ai-uploader-error.log",
        out_file: "./logs/ai-uploader-out.log",
        log_file: "./logs/ai-uploader-combined.log",
        time: true,
        max_memory_restart: "500M"
    }]
};
