import { useEffect, useState } from "react";
import Game from "./Game.tsx";
import { decode, diagnose, encode } from "cbor2";
function App() {
  const [name, setName] = useState("");
  const [ready, setReady] = useState(false);
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const submit = () => {
    console.log(name.trim());
    if (name.trim() != "") {
      socket?.send(encode({ event_type: "NewPlayer", name }));
      setReady(true);
    }
  };
  useEffect(() => {
    if (socket == null || socket.readyState !== socket.OPEN) return;

    console.log(socket !== null, "rerendering");
    socket?.send(encode({ event_type: "GetPlayersInLobby" }));
    // socket.addEventListener("open", () => {
    //   socket.send(encode({ event_type: "NewPlayer", name }));
    // });
    socket?.addEventListener("message", async (message) => {
      const blob: Blob = message.data;
      const arrBuff = new Uint8Array(await blob.arrayBuffer());
      const decoded = decode(arrBuff);
      console.log("message data", message.data, "decoded", decoded);
    });
  }, [socket?.readyState]);
  useEffect(() => {
    let tempSocket = new WebSocket("ws://localhost:6942");
    tempSocket.addEventListener("open", () => setSocket(tempSocket));
  }, []);
  return (
    <div className="flex h-full justify-center items-center">
      {!ready && (
        <div className="p-2 rounded border m-2 flex gap-2">
          <label className="flex gap-2">
            Enter your name:
            <input
              className=""
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                console.log(event.target.value);
              }}
            />
          </label>
          <button className="" onClick={() => submit()}>
            Ok
          </button>
        </div>
      )}
      {ready && <Game name={name} />}
    </div>
  );
}

export default App;
