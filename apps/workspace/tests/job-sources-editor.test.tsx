// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { JobSourcesEditor } from "../src/JobSources";

afterEach(cleanup);

it("says when every job is excluded and restores one", () => {
  const onRestore = vi.fn();
  render(
    <JobSourcesEditor
      jobs={[]}
      sources={{}}
      onChange={() => {}}
      ignored={["folder/E6759SM"]}
      onRestore={onRestore}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /E6759SM/ }));
  expect(onRestore).toHaveBeenCalledWith("folder/E6759SM");
  expect(screen.queryByRole("status")).toBeNull();
});
