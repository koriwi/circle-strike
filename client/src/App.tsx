import { useEffect, useMemo, useState } from "react";
import Game from "./Game.tsx";
import { decode, diagnose, encode } from "cbor2";
interface BaseEvent {
  event_type: string;
  [x: string]: unknown;
}
interface PlayerInLobby {
  id: string;
  name: string;
  color: string;
  ready: boolean;
  you: boolean;
}
function App() {
  const [name, setName] = useState("");
  const [ready, setReady] = useState(false);
  const [lobby, setLobby] = useState([] as PlayerInLobby[]);
  const [myId, setMyId] = useState("");
  const socket = useMemo(() => {
    return new WebSocket("ws://localhost:6942");
  }, []);
  const submit = () => {
    console.log(name.trim());
    if (name.trim() != "") {
      socket.send(encode({ event_type: "NewPlayer", name }));
      setReady(true);
    }
  };
  const toggleReady = () => {
    socket.send(encode({ event_type: "ToggleReady" }));
  };
  console.log("fn ready state socket.readyState");
  useEffect(() => {
    socket.addEventListener("message", async (message) => {
      const blob: Blob = message.data;
      const arrBuff = new Uint8Array(await blob.arrayBuffer());
      const decoded = decode(arrBuff) as BaseEvent;
      console.log("decoded", decoded);
      switch (decoded.event_type) {
        case "PlayersInLobby":
          setLobby(decoded.players as PlayerInLobby[]);
          break;
        case "YourId":
          setMyId(decoded.id.toString());
          break;
      }
    });
  }, []);

  return (
    <div className="flex flex-col h-full justify-center items-center gap-2">
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
          <div className="flex gap-4 border border-2 rounded items-center px-2 justify-between">
            <div className="flex items-center gap-2">
              <div
                style={{ backgroundColor: player.color.toLowerCase() }}
                className="size-4 border rounded"
              ></div>
              {player.name}
            </div>
            <div style={{ color: `${player.ready ? "green" : "gray"}` }}>
              {player.ready ? "ready" : "not ready"}
            </div>
          </div>
        ))}
      </div>
      {ready && (
        <div>
          <button onClick={() => toggleReady()}>
            {lobby.find((player) => player.id == myId)?.ready
              ? "set not ready"
              : "set ready"}
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
