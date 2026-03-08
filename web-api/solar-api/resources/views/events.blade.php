<x-main-layout>
<h1>Events</h1>
<div>Now: {{ $now }}</div>
<div style="margin-top: 20px;">
    @foreach ($events as $event)
        <div class="events-list-container">
            <div class="event-list-diff-col">{{ $now->diffInMinutes($event->timestamp, ['short' => true]) }}</div>
            <x-system-event style="display: grid; grid-template-columns: 60px auto" :event="$event->event" />
            <div class="event-list-diff-ts">{{$event->timestamp}}</div> 
            
        </div>
    @endforeach
</div>
</x-main-layout>
