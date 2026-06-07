import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import MaterialReviewPanel from "./MaterialReviewPanel";

const verifiedEntitlement = {
  data: { hasAccess: true, source: "purchases-db" },
  isLoading: false,
  isFetching: false,
  isError: false,
};

function renderPanel(props = {}) {
  return render(
    <MaterialReviewPanel
      materialId="mat-101"
      currentAddress="GBUYER1234567890"
      entitlement={verifiedEntitlement}
      initialReviews={[]}
      {...props}
    />,
  );
}

describe("MaterialReviewPanel", () => {
  it("renders the empty review state", () => {
    renderPanel({ initialReviews: [] });

    expect(screen.getByText("No reviews have been published yet.")).toBeInTheDocument();
    expect(screen.getByText("No reviews yet")).toBeInTheDocument();
  });

  it("renders selected stars and typed comment", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("radio", { name: "4 stars" }));
    fireEvent.change(screen.getByLabelText("Review"), {
      target: { value: "Clear, exam-ready explanations." },
    });

    expect(screen.getByRole("radio", { name: "4 stars" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByLabelText("Review")).toHaveValue("Clear, exam-ready explanations.");
  });

  it("shows accessible validation errors for invalid submit", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "Publish review" }));

    expect(screen.getByText("Choose a rating from 1 to 5 stars.")).toBeInTheDocument();
    expect(screen.getByText("Write a short review before publishing.")).toBeInTheDocument();
  });

  it("publishes a review and shows the verified buyer badge", async () => {
    renderPanel();

    fireEvent.click(screen.getByRole("radio", { name: "5 stars" }));
    fireEvent.change(screen.getByLabelText("Review"), {
      target: { value: "Excellent notes with useful diagrams and practice prompts." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Publish review" }));

    expect(screen.getByRole("button", { name: "Publishing review..." })).toBeDisabled();

    await waitFor(() => {
      expect(screen.getByText("Review published. Thanks for helping future learners choose well.")).toBeInTheDocument();
    });

    expect(screen.getByText("Excellent notes with useful diagrams and practice prompts.")).toBeInTheDocument();
    expect(screen.getByText("Verified buyer")).toBeInTheDocument();
  });

  it("handles unknown verification state gracefully", () => {
    renderPanel({
      currentAddress: "",
      entitlement: { data: null, isLoading: false, isFetching: false, isError: false },
    });

    expect(screen.getByText("Connect a wallet with a synced purchase to publish a verified review.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Publish review" })).toBeDisabled();
  });

  it("shows verified badges for verified review history", () => {
    renderPanel({
      initialReviews: [
        {
          id: "review-existing",
          rating: 5,
          comment: "Trusted purchase, strong worked examples.",
          reviewerAddress: "GVERIFIEDBUYER123456789",
          verifiedBuyer: true,
          createdAt: "2026-05-01T00:00:00.000Z",
        },
      ],
    });

    expect(screen.getByText("Trusted purchase, strong worked examples.")).toBeInTheDocument();
    expect(screen.getByText("Verified buyer")).toBeInTheDocument();
  });
});
