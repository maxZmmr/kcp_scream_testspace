import pandas as pd
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import os

def plot_scream_performance_with_loss():
    """
    Liest SCReAM-Leistungsdaten aus einer CSV-Datei und plottet die wichtigsten Metriken,
    um das Verhalten des Congestion-Control-Algorithmus zu visualisieren.
    Paketverlust-Events werden auf dem Bitraten-Graphen hervorgehoben.
    """
    try:
        # Lade die von deiner Rust-Anwendung erzeugten Log-Daten
        df = pd.read_csv('scream_log.csv')
        if df.empty:
            print("Fehler: scream_log.csv ist leer.")
            return

        # Konvertiere den Unix-Timestamp in ein lesbares Datumsformat für die X-Achse
        df['timestamp'] = pd.to_datetime(df['timestamp_ms'], unit='ms')

        # Finde alle Zeitpunkte, an denen ein Paketverlust auftrat
        loss_events = df[df['packet_loss'] == 1]

        # Erstelle eine Figur mit drei untereinander liegenden Graphen, die dieselbe Zeitachse teilen
        fig, (ax1, ax2, ax3) = plt.subplots(3, 1, figsize=(15, 12), sharex=True)
        fig.suptitle('SCReAM Congestion Control Behavior with Loss Indicators', fontsize=16)

        # --- Graph 1: Round-Trip Time (RTT) ---
        # Zeigt die Netzwerklatenz, einen wichtigen Input für den Algorithmus.
        ax1.plot(df['timestamp'], df['rtt_ms'], label='RTT (ms)', color='blue', alpha=0.7)
        ax1.set_ylabel('RTT (ms)')
        ax1.set_title('Round-Trip Time')
        ax1.grid(True)
        ax1.legend()

        # --- Graph 2: Ziel-Bitrate ---
        # Der wichtigste Graph zur Leistungsbeurteilung. Zeigt den Output des Algorithmus.
        ax2.plot(df['timestamp'], df['bitrate_kbps'], label='Target Bitrate (kbps)', color='green')
        # Zeichne auffällige Marker für jeden Paketverlust, um die Reaktion des Algorithmus zu sehen.
        ax2.scatter(loss_events['timestamp'], loss_events['bitrate_kbps'],
                    color='gold',
                    edgecolor='black',
                    s=100,
                    zorder=5,
                    label='Packet Loss Event')
        ax2.set_ylabel('Bitrate (kbps)')
        ax2.set_title('Target Bitrate')
        ax2.grid(True)
        ax2.legend()

        # --- Graph 3: Congestion Window (CWND) ---
        # Zeigt die interne Stellschraube, die der Bitrate zugrunde liegt.
        ax3.plot(df['timestamp'], df['cwnd_bytes'], label='CWND (Bytes)', color='red', alpha=0.8)
        ax3.set_ylabel('CWND (Bytes)')
        ax3.set_title('Calculated Congestion Window')
        ax3.grid(True)
        ax3.legend()

        # Formatiere die X-Achse, damit die Zeit lesbar ist
        ax3.set_xlabel('Time')
        ax3.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        fig.autofmt_xdate()

        # Sorge für ein sauberes Layout und speichere die Grafik
        plt.tight_layout(rect=[0, 0, 1, 0.96])
        plt.savefig('scream_performance_plot.png')
        
        print("Grafik wurde erfolgreich als 'scream_performance_plot.png' gespeichert.")

        # Optional: Log-Datei nach der Visualisierung entfernen
        # os.remove("scream_log.csv")

    except FileNotFoundError:
        print("Fehler: Die Datei 'scream_log.csv' wurde nicht gefunden.")
        print("Bitte führen Sie zuerst das Rust-Testprogramm aus, um die Log-Datei zu erzeugen.")
    except Exception as e:
        print(f"Ein unerwarteter Fehler ist aufgetreten: {e}")

if __name__ == '__main__':
    plot_scream_performance_with_loss()