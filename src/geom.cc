#include "geom.h"
#include "draw.h"

#include <map>

void Mesh::normalize(double)
{
	assert(mTriangles.size() > 0);

	static Coord3<double> min, max, d, c;
	static double maxd;

	min = mTriangles[0].v1.vertex;
	max = mTriangles[0].v1.vertex;

	std::vector<Triangle>::iterator i;

	for(i = mTriangles.begin(); i != mTriangles.end(); ++i)
		for (int j = 0; j < 3; j++)
		{
			Vertex &v = (*i)[j];
			if (min.x > v.vertex.x) min.x = v.vertex.x;
			if (min.y > v.vertex.y) min.y = v.vertex.y;
			if (min.z > v.vertex.z) min.z = v.vertex.z;
			if (max.x < v.vertex.x) max.x = v.vertex.x;
			if (max.y < v.vertex.y) max.y = v.vertex.y;
			if (max.z < v.vertex.z) max.z = v.vertex.z;
		}

	d = max - min;
	maxd = d.x;
	if (maxd < d.y) maxd = d.y;
	if (maxd < d.z) maxd = d.z;

	c = (min+max) / 2;
	for(i = mTriangles.begin(); i != mTriangles.end(); ++i)
		for (int j = 0; j < 3; j++)
		{
			(*i)[j].vertex -= c;
			(*i)[j].vertex /= maxd;
		}
}

void Mesh::smooth()
{
	std::map< Coord3<double>,Coord3<double> > normal_map;
	std::vector<Triangle>::iterator i;
	static Coord3<double> zero(0, 0, 0);

	for (i = mTriangles.begin(); i != mTriangles.end(); ++i)
		for (int j=0; j < 3; j++)
			normal_map[(*i)[j].vertex].set(zero);

	for (i = mTriangles.begin(); i != mTriangles.end(); ++i)
		for (int j=0; j < 3; j++)
			normal_map[(*i)[j].vertex] += (*i)[j].normal;

	std::map<Coord3<double>,Coord3<double> >::iterator m;
	for (m = normal_map.begin(); m != normal_map.end(); ++m)
		(*m).second.normalize();

	for (i = mTriangles.begin(); i != mTriangles.end(); ++i)
		for (int j = 0; j < 3; j++)
			(*i)[j].normal = normal_map[(*i)[j].vertex];
}


void Mesh::setMapMode(MapMode mode)
{
	std::vector<Triangle>::iterator i;

	static Coord3<double> tp(.5, .5, 0);
	static Coord2<double> tmp;

	for (i = mTriangles.begin(); i != mTriangles.end(); ++i)
	{
		for (int j=0; j<3; j++)
		{
			Vertex &v = (*i)[j];
			switch (mode)
			{
			case PLANAR:
				tmp.set(v.vertex.x + tp.x, v.vertex.y + tp.y);
				v.texture = tmp;
				break;

			case CYLINDRICAL:
				v.texture.x = (1+atan2(v.vertex.x, v.vertex.y)/M_PI)/2;
				v.texture.y = v.vertex.z + .5;
				break;
			}
		}
	}
}

extern unsigned int ASTEROID_TEXTURE;

void Mesh::draw()
{
	if (mHasTexture)
	{
		glEnable(GL_TEXTURE_2D);
		glBindTexture(GL_TEXTURE_2D, ASTEROID_TEXTURE);
		glTexEnvi(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_MODULATE);
		draw::setColor(1, 1, 1, 1);
	}

	glBegin(GL_TRIANGLES);

	std::vector<Triangle>::iterator i;
	for (i = mTriangles.begin(); i != mTriangles.end(); ++i)
	{
		for (std::size_t j = 0; j < 3; j++)
		{
			Vertex &v = (*i)[j];
			if (mHasTexture) glTexCoord2d(v.texture.x, v.texture.y);
			glNormal3d(v.normal.x, v.normal.y, v.normal.z);
			glVertex3d(v.vertex.x, v.vertex.y, v.vertex.z);
		}
	}
	glEnd();

	if (mHasTexture)
		glDisable(GL_TEXTURE_2D);
}
