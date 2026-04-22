# Dissertation Evaluation Output

- scale: 1
- seed: 42
- scenarios: 41
- warmup windows: 80

## Files
- summary.csv
- warmup_curve.csv
- evaluation.json
- figure_index.csv
- figure_index.md
- evaluation_interpretation.md
- graph_explanations.md
- must_include_report_summary.md
- dissertation_figure_table.md
- plots generated: 49

## Plot files
- plots/overview_hit_rate.png
- plots/overview_byte_hit_rate.png
- plots/overview_p95_latency_ms.png
- plots/overview_throughput_rps.png
- plots/admission_policies_hit_rate.png
- plots/admission_policies_p95_latency.png
- plots/admission_policies_throughput.png
- plots/baseline_vs_cdn_ab_toggle_hit_rate.png
- plots/baseline_vs_cdn_ab_toggle_p95_latency.png
- plots/baseline_vs_cdn_ab_toggle_throughput.png
- plots/cold_to_warm_sanity_hit_rate.png
- plots/cold_to_warm_sanity_p95_latency.png
- plots/cold_to_warm_sanity_throughput.png
- plots/event_driven_invalidation_correctness_hit_rate.png
- plots/event_driven_invalidation_correctness_p95_latency.png
- plots/event_driven_invalidation_correctness_throughput.png
- plots/policy_capacity_sweep_hit_rate.png
- plots/policy_capacity_sweep_p95_latency.png
- plots/policy_capacity_sweep_throughput.png
- plots/scan_resistance_hit_rate.png
- plots/scan_resistance_p95_latency.png
- plots/scan_resistance_throughput.png
- plots/size_aware_admission_adaptsize_lite_hit_rate.png
- plots/size_aware_admission_adaptsize_lite_p95_latency.png
- plots/size_aware_admission_adaptsize_lite_throughput.png
- plots/stress_concurrency_scaling_hit_rate.png
- plots/stress_concurrency_scaling_p95_latency.png
- plots/stress_concurrency_scaling_throughput.png
- plots/cold_start_warmup_curve_line.png
- plots/concurrency_scaling_throughput_line.png
- plots/concurrency_scaling_hit_rate_line.png
- plots/concurrency_scaling_avg_latency_line.png
- plots/baseline_relative_throughput_speedup.png
- plots/baseline_relative_latency_speedup.png
- plots/latency_cdf_key_variants.png
- plots/policy_tradeoff_avg_latency_vs_hit_rate.png
- plots/policy_tradeoff_p95_vs_hit_rate.png
- plots/normalized_composite_score.png
- plots/must_include/overview_hit_rate.png
- plots/must_include/overview_p95_latency_ms.png
- plots/must_include/baseline_relative_throughput_speedup.png
- plots/must_include/baseline_relative_latency_speedup.png
- plots/must_include/admission_policies_hit_rate.png
- plots/must_include/scan_resistance_hit_rate.png
- plots/must_include/cold_start_warmup_curve_line.png
- plots/must_include/concurrency_scaling_throughput_line.png
- plots/must_include/latency_cdf_key_variants.png
- plots/must_include/policy_tradeoff_avg_latency_vs_hit_rate.png
- plots/must_include/policy_tradeoff_p95_vs_hit_rate.png

## Dissertation Must-Include Figures
These are pre-selected under `plots/must_include/` as the minimum core set for the evaluation section:
- overview_hit_rate.png
- overview_p95_latency_ms.png
- baseline_relative_throughput_speedup.png
- baseline_relative_latency_speedup.png
- admission_policies_hit_rate.png
- scan_resistance_hit_rate.png
- cold_start_warmup_curve_line.png
- concurrency_scaling_throughput_line.png
- latency_cdf_key_variants.png
- policy_tradeoff_avg_latency_vs_hit_rate.png
- policy_tradeoff_p95_vs_hit_rate.png

## Utility Verdict
Current synthetic evaluation supports that the cache/CDN-like layer is useful and performance-improving.
