import type { AnchorHTMLAttributes, MouseEvent, ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage } from "../runtime/errors";

type ExternalUrlLinkProps = Omit<
  AnchorHTMLAttributes<HTMLAnchorElement>,
  "href" | "onClick" | "children"
> & {
  href: string;
  children?: ReactNode;
};

export function ExternalUrlLink({
  href,
  children,
  ...anchorProps
}: ExternalUrlLinkProps) {
  async function openExternalUrl(event: MouseEvent<HTMLAnchorElement>) {
    event.preventDefault();
    try {
      await openUrl(href);
    } catch (error) {
      window.alert(errorMessage(error));
    }
  }

  return (
    <a
      {...anchorProps}
      href={href}
      target="_blank"
      rel="noreferrer"
      onClick={(event) => void openExternalUrl(event)}
    >
      {children}
    </a>
  );
}
