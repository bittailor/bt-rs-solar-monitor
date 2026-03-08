<x-main-layout>
<h1>Readings</h1>
<div>Now: {{ $now }}</div>
<div style="margin-top: 20px;">
    <table class="table table-sm">
        <thead>
            <tr>
            <th scope="col">Bat V</th>
            <th scope="col">Bat I</th>
            <th scope="col">Pan V</th>
            <th scope="col">Pan W</th>
            <th scope="col">Loa I</th>
            <th scope="col">TS</th>
            </tr>
        </thead>
        <tbody>
            @foreach ($readings as $reading)
                <tr>
                    <td>{{ $reading->battery_voltage }}</td>
                    <td>{{ $reading->battery_current }}</td>
                    <td>{{ $reading->panel_voltage }}</td>
                    <td>{{ $reading->panel_power }}</td>
                    <td>{{ $reading->load_current }}</td>
                    <td style="font-size: xx-small;">{{ $reading->recorded_at }}</td>
                </tr>
            @endforeach
        </tbody>
    </table>
</div>
</x-main-layout>