/**
 * Entry redirect -- sends the user to login or main based on auth state.
 */
import { Redirect } from "expo-router";
import { ActivityIndicator, Image, View, StyleSheet, Text } from "react-native";
import { useAuthStore } from "../src/store/auth";

export default function Index() {
  const isReady = useAuthStore((s) => s.isReady);
  const isLoading = useAuthStore((s) => s.isLoading);
  const statusText = useAuthStore((s) => s.statusText);

  if (isLoading) {
    return (
      <View style={styles.center}>
        <Image source={require("../assets/icon.png")} style={styles.logo} />
        <ActivityIndicator size="small" color="#CAC4D0" style={styles.spinner} />
        {statusText ? <Text style={styles.status}>{statusText}</Text> : null}
      </View>
    );
  }

  if (isReady) {
    return <Redirect href="/(main)/conversations" />;
  }

  return <Redirect href="/login" />;
}

const styles = StyleSheet.create({
  center: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    backgroundColor: "#ffffff",
  },
  logo: {
    width: 120,
    height: 120,
    marginBottom: 32,
  },
  spinner: {
    marginBottom: 16,
  },
  status: {
    color: "#938F99",
    fontSize: 14,
  },
});
