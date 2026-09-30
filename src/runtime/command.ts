import { commands } from "./bindings";

type CommandName = keyof typeof commands;
type CommandArguments<Name extends CommandName> = Parameters<(typeof commands)[Name]>;
type CommandResult<Name extends CommandName> = ReturnType<(typeof commands)[Name]>;

/** Invoke a generated Tauri command by its generated wrapper name. */
export function command<Name extends CommandName>(
  name: Name,
  ...args: CommandArguments<Name>
): CommandResult<Name> {
  return Reflect.apply(commands[name], undefined, args) as CommandResult<Name>;
}
