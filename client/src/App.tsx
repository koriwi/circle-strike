import { useEffect, useState } from "react";
import Game from "./Game.tsx";
import { decode, diagnose, encode } from "cbor2";
interface BaseEvent {
  event_type: string;
  [x: string]: unknown;
}
interface PlayerInLobby {
  name: string;
  color: string;
}
function App() {
  const [name, setName] = useState("");
  const [ready, setReady] = useState(false);
  const [lobby, setLobby] = useState([] as PlayerInLobby[]);
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
      const decoded = decode(arrBuff) as BaseEvent;
      switch (decoded.event_type) {
        case "PlayersInLobby":
          setLobby(decoded.players as PlayerInLobby[]);
          break;
      }
    });
  }, [socket?.readyState]);

  useEffect(() => {
    let tempSocket = new WebSocket("ws://localhost:6942");
    tempSocket.addEventListener("open", () => setSocket(tempSocket));
  }, []);

  return (
    <div className="flex flex-col h-full justify-center items-center">
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
      <div className="flex flex-col gap-2 p-2 border rounded ">
        Players:
        {lobby.map((player) => (
          <div className="flex gap-2 border rounded items-center px-2">
            <div
              style={{ backgroundColor: player.color.toLowerCase() }}
              className="size-4 border rounded"
            ></div>
            {player.name}
          </div>
        ))}
      </div>
      {/* {ready && <Game name={name} />} */}
    </div>
  );
}

export default App;
