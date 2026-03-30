/**
 * Skeleton loading placeholder for the conversation list.
 * Mirrors the ConversationListItem layout with animated shimmer placeholders.
 */
import React, { useEffect, useRef } from "react";
import { View, StyleSheet, Animated } from "react-native";

const SKELETON_COUNT = 8;

function SkeletonRow({ opacity }: { opacity: Animated.Value }) {
  return (
    <Animated.View style={[styles.row, { opacity }]}>
      <View style={styles.avatar} />
      <View style={styles.body}>
        <View style={styles.titleBar} />
        <View style={styles.previewBar} />
      </View>
      <View style={styles.timeBar} />
    </Animated.View>
  );
}

export function ConversationListSkeleton() {
  const opacity = useRef(new Animated.Value(0.3)).current;

  useEffect(() => {
    const animation = Animated.loop(
      Animated.sequence([
        Animated.timing(opacity, {
          toValue: 0.6,
          duration: 800,
          useNativeDriver: true,
        }),
        Animated.timing(opacity, {
          toValue: 0.3,
          duration: 800,
          useNativeDriver: true,
        }),
      ])
    );
    animation.start();
    return () => animation.stop();
  }, []);

  return (
    <View style={styles.container}>
      {Array.from({ length: SKELETON_COUNT }, (_, i) => (
        <SkeletonRow key={i} opacity={opacity} />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  row: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  avatar: {
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: "#2B2930",
    marginRight: 16,
  },
  body: {
    flex: 1,
    justifyContent: "center",
    marginRight: 12,
    gap: 8,
  },
  titleBar: {
    width: "60%",
    height: 14,
    borderRadius: 4,
    backgroundColor: "#2B2930",
  },
  previewBar: {
    width: "85%",
    height: 12,
    borderRadius: 4,
    backgroundColor: "#2B2930",
  },
  timeBar: {
    width: 36,
    height: 10,
    borderRadius: 4,
    backgroundColor: "#2B2930",
  },
});
