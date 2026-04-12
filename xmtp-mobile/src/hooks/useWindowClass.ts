import { useMemo } from "react";
import { useWindowDimensions } from "react-native";

export type WindowClass = "compact" | "medium" | "expanded";

export function useWindowClass(): WindowClass {
  const window = useWindowDimensions();

  return useMemo(() => {
    if (window.width >= 840) return "expanded";
    if (window.width >= 600) return "medium";
    return "compact";
  }, [window.width]);
}
