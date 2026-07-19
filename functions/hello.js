globalThis.handler = function(request) {
    return JSON.stringify({
        status: 200,
        body: "hello from js function",
        request: request
    });
};
