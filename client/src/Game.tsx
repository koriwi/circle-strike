import { decode, diagnose, encode } from "cbor2";
import type React from "react";
import { useEffect } from "react";

const Game: React.FC<{ name: string }> = function ({ name }) {
  return (
    <div className="flex justify-between">
      <div>Game</div>
      <div>{name}</div>
    </div>
  );
};
export default Game;
