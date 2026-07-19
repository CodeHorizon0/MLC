globalThis.handler = function(request: string): string {
    return JSON.stringify({
        status: 200,
        body: "hello from typescript",
        request: request
    });
};
