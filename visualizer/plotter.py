# in visualizer/plotter.py

import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import os

def plot_scream_performance():
    try:
        df = pd.read_csv('scream_log.csv')
        if df.empty:
            print("Fehler: scream_log.csv ist leer.")
            return

        df['timestamp'] = pd.to_datetime(df['timestamp_ms'], unit='ms')
        loss_events = df[df['packet_loss'] == 1]

        fig, (ax1, ax2, ax3, ax4) = plt.subplots(4, 1, figsize=(15, 16), sharex=True)
        fig.suptitle('SCReAM v2 Performance Analysis', fontsize=16)

        # --- Graph 1: RTT Metrics ---
        ax1.plot(df['timestamp'], df['s_rtt_ms'], label='Smoothed RTT (ms)', color='blue')
        ax1.plot(df['timestamp'], df['base_rtt_ms'], label='Base RTT (ms)', color='cyan', linestyle='--')
        ax1.set_ylabel('RTT (ms)')
        ax1.set_title('RTT Metrics')
        ax1.grid(True)
        ax1.legend()

        # --- Graph 2: Queueing Delay ---
        ax2.plot(df['timestamp'], df['qdelay_avg_ms'], label='Smoothed Queue Delay (ms)', color='orange')
        ax2.axhline(y=60, color='r', linestyle='--', label='Queue Delay Target (60ms)')
        ax2.set_ylabel('Queue Delay (ms)')
        ax2.set_title('Queueing Delay vs. Target')
        ax2.grid(True)
        ax2.legend()
        
        # --- Graph 3: Target Bitrate and Loss Events ---
        ax3.plot(df['timestamp'], df['bitrate_kbps'], label='Target Bitrate (kbps)', color='green')
        ax3.scatter(loss_events['timestamp'], loss_events['bitrate_kbps'],
                    color='red', edgecolor='black', s=100, zorder=5, label='Packet Loss Event')
        ax3.set_ylabel('Bitrate (kbps)')
        ax3.set_title('Target Bitrate and Packet Loss')
        ax3.grid(True)
        ax3.legend()

        # --- Graph 4: Congestion Window and Bytes in Flight ---
        ax4.plot(df['timestamp'], df['cwnd_bytes'], label='CWND (Bytes)', color='purple')
        ax4.plot(df['timestamp'], df['bytes_in_flight'], label='Bytes in Flight', color='gray', alpha=0.7)
        ax4.plot(df['timestamp'], df['max_bytes_in_flight'], label='Max Bytes in Flight (last RTT)', color='pink', linestyle=':')
        ax4.set_ylabel('Bytes')
        ax4.set_title('Congestion Window and In-Flight Data')
        ax4.grid(True)
        ax4.legend()

        ax4.set_xlabel('Time')
        ax4.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        fig.autofmt_xdate()

        plt.tight_layout(rect=[0, 0, 1, 0.96])
        plt.savefig('scream_performance_analysis.png')

        os.remove('scream_log.csv')
        
        print("Grafik wurde erfolgreich als 'scream_performance_analysis.png' gespeichert.")

    except FileNotFoundError:
        print("Fehler: Die Datei 'scream_log.csv' wurde nicht gefunden.")
    except Exception as e:
        print(f"Ein unerwarteter Fehler ist aufgetreten: {e}")

if __name__ == '__main__':
    plot_scream_performance()