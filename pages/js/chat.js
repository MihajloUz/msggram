const socket = new WebSocket("ws://localhost:3000/ws");

socket.addEventListener('open', (event) => {
    console.log("Didy");
});
