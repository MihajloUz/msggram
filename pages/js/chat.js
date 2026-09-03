const protocol = location.protocol === "https:" ? "wss:" : "ws:";
const socket = new WebSocket(`${protocol}//${location.host}/ws`);

socket.onopen = () => {
    console.log("WebSocket connected");
};

socket.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    const h1 = document.createElement("h1");
    h1.textContent = msg.contents;
    h1.style.color = "blue";

    document.body.appendChild(h1);
};

socket.onclose = () => {
    console.log("WebSocket disconnected");
};

socket.onerror = (error) => {
    console.error("WebSocket error:", error);
};

let receiverId = null;

document.querySelectorAll(".user").forEach(button => {
    button.addEventListener("click", () => {
        receiverId = button.dataset.userId;
    });
});

document.getElementById("send_button").addEventListener("click", () => {
    if (receiverId === null) {
        return;
    }


    const h1 = document.createElement("h1");
    h1.textContent = document.getElementById("msg_input").value;
    document.body.appendChild(h1);
    h1.style.color = "red";

    socket.send(JSON.stringify({
        receiver_id: receiverId,
        contents: document.getElementById("msg_input").value
    }));
});
