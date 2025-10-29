from sql2viz import VizBuilder

builder = VizBuilder()

# タブ1: 月別売上（折れ線グラフ）
builder.add_query('''
    SELECT 
        ['Jan','Feb','Mar','Apr','May','Jun',
         'Jul','Aug','Sep','Oct','Nov','Dec'][generate_series] as month,
        (50 + random() * 50)::INT as revenue,
        (20 + random() * 30)::INT as profit
    FROM generate_series(1, 12)
''')
builder.with_chart('Line', 'month', 'revenue')

# タブ2: カテゴリ別販売数（棒グラフ）
builder.add_query('''
    SELECT 
        ['Electronics', 'Clothing', 'Food', 'Books', 'Toys'][generate_series] as category,
        (100 + random() * 200)::INT as sales
    FROM generate_series(1, 5)
''')
builder.with_chart('Bar', 'category', 'sales')

# タブ3: 時系列データ（波形）
builder.add_query('''
    SELECT 
        generate_series as day,
        (50 + 10 * sin(generate_series * 0.5) + random() * 10)::INT as temperature
    FROM generate_series(1, 20)
''')
builder.with_chart('Line', 'day', 'temperature')

# タブ4: 散布図
builder.add_query('''
    SELECT 
        (random() * 100)::INT as x,
        (random() * 100)::INT as y,
        (random() * 50)::INT as size
    FROM generate_series(1, 30)
''')
builder.with_chart('Scatter', 'x', 'y')

# 1つのウィンドウに4つのタブとして表示
builder.launch()